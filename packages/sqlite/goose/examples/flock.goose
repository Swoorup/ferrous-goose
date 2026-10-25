// SQLite from Goose, with no Rust on the machine: goose run flock.goose
import sqlite from "..";

fn main() {
    var err: u8[>..] = [];
    let db, ok = sqlite::open_memory(err);
    guard ok else { abort(err); }
    assert(sqlite::exec(db, "create table geese (name text, honks integer, weight real)"));
    let names: const u8[:][3] = ["Gerald", "Agnes", "Mabel"];
    for i in 3 {
        let p = sqlite::params();
        sqlite::bind_text(p, names[i]);
        sqlite::bind_int(p, (i + 1) * 10);
        sqlite::bind_real(p, 3.5 + i as f64);
        let n, eok = sqlite::execute(db, "insert into geese values (?, ?, ?)", p);
        assert(eok && n == 1);
        sqlite::release(p);
    }
    print("last rowid ", sqlite::last_insert_rowid(db));

    let rows, qok = sqlite::select(db, "select name, honks, weight from geese order by honks desc");
    guard qok else { abort("query failed"); }
    print(sqlite::row_count(rows), " rows, ", sqlite::columns(rows), " columns");
    while sqlite::step(rows) {
        let name, nok = sqlite::get_text(rows, 0);
        let honks, hok = sqlite::get_int(rows, 1);
        let w, wok = sqlite::get_real(rows, 2);
        print(name, ": ", honks, " honks, ", w, " kg");
    }
    sqlite::release(rows);

    var qerr: u8[>..] = [];
    let bad, bok = sqlite::select(db, "select nope from geese", qerr);
    print("bad query: ", bok, ", says why: ", qerr.len > 0);
    sqlite::release(db);
    print("live: ", sqlite::live_count());
}
