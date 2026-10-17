mod outer {
    pub struct Thing {
        needle: u32,
    }

    impl Thing {
        pub fn helper(&self) -> u32 {
            self.needle + 1
        }
    }

    pub trait Greeter {
        fn greet(&self) -> String {
            String::from("needle")
        }
    }
}
