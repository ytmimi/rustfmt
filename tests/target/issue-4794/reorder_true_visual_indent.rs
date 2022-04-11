// rustfmt-imports_indent: Visual
// rustfmt-reorder_imports: true

use module::{submodule_A::{Type_A1, Type_A2},
             submodule_B::{Type_B1, Type_B2}};

pub use module::{submodule_A::{Type_A1, Type_A2},
                 submodule_B::{Type_B1, Type_B2}};

pub(crate) use module::{submodule_A::{Type_A1, Type_A2},
                        submodule_B::{Type_B1, Type_B2}};

use module::{submodule_A::{Type_A1, Type_A2},
             submodule_B::{Type_B1, Type_B2}};

pub use module::{submodule_A::{Type_A1, Type_A2},
                 submodule_B::{Type_B1, Type_B2}};

pub(crate) use module::{submodule_A::{Type_A1, Type_A2},
                        submodule_B::{Type_B1, Type_B2}};

// deeply nested use
mod nested {
    mod even_more_nested {
        use module::{submodule_A::{Type_A1, Type_A2},
                     submodule_B::{Type_B1, Type_B2}};

        pub use module::{submodule_A::{Type_A1, Type_A2},
                         submodule_B::{Type_B1, Type_B2}};

        pub(crate) use module::{submodule_A::{Type_A1, Type_A2},
                                submodule_B::{Type_B1, Type_B2}};

        use module::{submodule_A::{Type_A1, Type_A2},
                     submodule_B::{Type_B1, Type_B2}};

        pub use module::{submodule_A::{Type_A1, Type_A2},
                         submodule_B::{Type_B1, Type_B2}};

        pub(crate) use module::{submodule_A::{Type_A1, Type_A2},
                                submodule_B::{Type_B1, Type_B2}};
    }
}

// use inside a function
fn main() {
    use module::{submodule_A::{Type_A1, Type_A2},
                 submodule_B::{Type_B1, Type_B2}};
    println!("hello world!");
}
