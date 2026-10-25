// A panic in a function that cannot fail ends the program as an abort does.
import cf;

fn main() {
    print(cf::panics(1));
    print(cf::panics(-1));
    print("not reached");
}
