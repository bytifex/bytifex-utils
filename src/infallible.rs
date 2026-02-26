use std::convert::Infallible;

pub trait UnwrapInfallible<T> {
    fn infallible(self) -> T;
}

impl<T> UnwrapInfallible<T> for Result<T, Infallible> {
    fn infallible(self) -> T {
        self.unwrap_or_else(|never| match never {})
    }
}
