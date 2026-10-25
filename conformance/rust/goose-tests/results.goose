// Option and Result results, with and without the error text; a panic in a
// fallible function is its failure; throws through the exception module.
import cf;
import exception;

fn strict_calls() {
    print("strict ", cf::strict_parse("99"));
    print("never ", cf::strict_parse("nope"));
}

fn main() {
    let q, ok = cf::checked_div(42, 6);
    print("div ", q, " ", ok);
    let q2, ok2 = cf::checked_div(1, 0);
    print("div by zero ", ok2);

    var err: u8[>..] = [];
    let v, ok3 = cf::parse_int(" 1234 ", err);
    print("parsed ", v, " ", ok3);
    let v2, ok4 = cf::parse_int("12x", err);
    print("bad int ", ok4, ": ", err);
    print("positive ", cf::must_be_positive(5));
    var e2: u8[>..] = [];
    print("not positive ", cf::must_be_positive(-5, e2), ": ", e2);

    let h, ok5 = cf::parse_hit("2.5, 77");
    print("hit ", h.id, " ", h.t, " ", ok5);
    let name, ok6 = cf::maybe_name(3);
    print("name ", name, " ", ok6);
    let name2, ok7 = cf::maybe_name(0);
    print("no name ", ok7, " ", name2.len);

    var pe: u8[>..] = [];
    let p, ok8 = cf::panics_fallible(-3, pe);
    print("panic as failure: ", ok8, ": ", pe[0..24]);

    exception::try<u8[]>(strict_calls) { error => print("caught: ", error); };
}
