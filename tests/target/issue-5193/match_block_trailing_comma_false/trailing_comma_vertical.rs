// rustfmt-max_width: 90
// rustfmt-match_block_trailing_comma: false
// rustfmt-trailing_comma: Vertical

fn main() {
    match FooBar::Foo {
        FooBar::Foo => unreachable!(),
        FooBar::Bar => {
            println!("match_block_trailing_comma: false");
            println!("trailing_comma: Vertical");
        }
        FooBar::Baz => {
            println!("Lorem ipsum dolor sit amet, consectetuer adipiscing elit.")
        }
    }
}
