#!/usr/bin/env python3
"""Exercise native-build providers with plain C, without Cargo or Rust."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


PROVIDER = r'''
import argparse, json, pathlib, subprocess
p = argparse.ArgumentParser()
for flag in ("out-dir", "cache-dir", "compiler", "include-dir"):
    p.add_argument("--" + flag, required=True)
p.add_argument("--mode", default="ok")
a = p.parse_args()
cache = pathlib.Path(a.cache_dir)
cache.mkdir(parents=True, exist_ok=True)
source = cache / "native.c"
source.write_text("""
#include "goose.h"
static int initialized;
#if defined(__linux__) && defined(__x86_64__)
/* A PIC object may still have assembly PC-relative references to a global
   label. The archive must bind that label locally when linked into the JIT. */
__asm__(
    ".text\\n"
    ".globl native_trampoline_return\\n"
    ".type native_trampoline_return, @function\\n"
    "native_trampoline_return: ret\\n"
    ".globl native_trampoline_address\\n"
    ".type native_trampoline_address, @function\\n"
    "native_trampoline_address: lea native_trampoline_return(%rip), %rax; ret\\n"
);
extern void *native_trampoline_address(void);
#else
static void *native_trampoline_address(void) { return &initialized; }
#endif
void example_start(const gs_rt_hooks *hooks) {
    initialized = hooks && hooks->version >= 1 && native_trampoline_address() != 0;
}
int64_t example_answer(void) { return initialized ? 42 : -1; }
void example_squares(int64_t n, gs_rref out) {
    for (int64_t i = 0; i < n; ++i) {
        int64_t value = i * i;
        gs_bld_push(out, &value, 1, sizeof value);
    }
}
""")
obj, archive = cache / "native.o", cache / "libnative.a"
subprocess.run(["cc", "-fPIC", "-I", a.include_dir, "-c", str(source), "-o", str(obj)], check=True)
subprocess.run(["ar", "rcs", str(archive), str(obj)], check=True)
(pathlib.Path(a.out_dir) / "native.goose").write_text("""
namespace native;
extern fn answer() -> i64 = "example_answer";
extern fn squares(n: i64, out: i64[>..]&) = "example_squares";
""")
report = {"version": 1, "staticlib": str(archive), "runtime_hooks": "example_start"}
if a.mode == "version": report["version"] = 99
if a.mode == "missing": report["staticlib"] = str(cache / "missing.a")
if a.mode == "hook": report["runtime_hooks"] = "bad-name"
if a.mode == "combine": report["combine"] = ["unused-combiner"]
print("goose-native: " + json.dumps(report))
'''


def main():
    compiler = Path(sys.argv[1] if len(sys.argv) > 1 else ".toolchain/goose/build/goose").resolve()
    with tempfile.TemporaryDirectory(prefix="goose native provider ") as temp:
        root = Path(temp)
        provider = root / "provider.py"
        provider.write_text(PROVIDER)
        program = root / "main.goose"
        program.write_text("""import native;
fn main() {
    print(native::answer());
    var xs: i64[>..] = [100];
    native::squares(4, xs);
    print(xs);
}
""")
        env = dict(os.environ, GOOSE_HOME=str(root / "cache"), GOOSE_PROGRESS="plain")
        # The provider and compiler are absolute paths; their tools are system C tools.
        env["PATH"] = "/usr/bin:/bin"
        def configure(mode):
            argv = [sys.executable, str(provider), "--mode", mode]
            (root / "main.build").write_text("native_build = " + json.dumps(argv) + ";\n")
        def goose(*args):
            return subprocess.run([str(compiler), *map(str, args)], cwd=root, env=env, text=True, capture_output=True)
        configure("ok")
        expected = "42\n[100, 0, 1, 4, 9]\n"
        result = goose("run", program)
        assert result.returncode == 0 and result.stdout == expected, result.stdout + result.stderr
        executable = root / "example"
        result = goose("build", program, "-o", executable)
        assert result.returncode == 0, result.stdout + result.stderr
        result = subprocess.run([str(executable)], env=env, text=True, capture_output=True)
        assert result.returncode == 0 and result.stdout == expected, result.stdout + result.stderr
        for mode, diagnostic in [("version", "protocol version 1"), ("missing", "missing library"),
                                 ("hook", "public C identifier"), ("combine", "both combine and combine_input")]:
            configure(mode)
            result = goose("check", program)
            assert result.returncode != 0 and diagnostic in result.stderr, result.stdout + result.stderr
    print("native providers: C JIT/AOT, hooks, typed builders and 4 rejection cases passed")


if __name__ == "__main__":
    main()
