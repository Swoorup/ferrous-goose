// Optional parameters: an entry point per combination, and a wrapper that
// narrows the Goose optionals.
import cf;

fn main() {
    var cfg = cf::Config { level: 3, verbose: true };
    print(cf::describe_config(cfg));
    print(cf::describe_config(null));
    var b: i64 = 2;
    var c: i64 = 3;
    print(cf::add_opt(1, b, c), " ", cf::add_opt(1, b, null), " ", cf::add_opt(1, null, c), " ", cf::add_opt(1, null, null));
    var name: const u8[:] = "goose";
    print(cf::greet(name));
    print(cf::greet(null));
}
