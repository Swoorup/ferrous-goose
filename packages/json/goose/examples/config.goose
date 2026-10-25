// serde_json from Goose, with no Rust on the machine: goose run config.goose
import sjson from "..";

fn main() {
    var err: u8[>..] = [];
    let doc, ok = sjson::parse("{\"name\": \"goose\", \"legs\": 2, \"tags\": [\"fast\", \"loud\"], \"safe\": true}", err);
    guard ok else { abort(err); }
    let name, nok = sjson::get(doc, "name");
    let text, tok = sjson::as_str(name);
    print("name: ", text);
    let legs, lok = sjson::get(doc, "legs");
    let n, iok = sjson::as_i64(legs);
    print("legs: ", n);
    let tag, pok = sjson::pointer(doc, "/tags/1");
    let t, sok = sjson::as_str(tag);
    print("second tag: ", t);
    sjson::each_keys(doc) { k => print("key: ", k); };

    let more = sjson::integer(3);
    sjson::set(doc, "eggs", more);
    print(sjson::stringify(doc));

    var bad: u8[>..] = [];
    let b, bok = sjson::parse("{oops", bad);
    print("bad json: ", bok, ", an error: ", bad.len > 0);
    sjson::release(name);
    sjson::release(legs);
    sjson::release(tag);
    sjson::release(more);
    sjson::release(doc);
    print("live: ", sjson::live_count());
}
