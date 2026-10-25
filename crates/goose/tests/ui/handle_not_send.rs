#[goose::handle]
pub struct Shared(std::rc::Rc<i64>);

fn main() {}
