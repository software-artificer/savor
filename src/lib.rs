use std::ops;

/// Function to add two numbers together. It is generic so supports any types that implement the
/// [std::ops::Add] macro.
pub fn sum<T>(a: T, b: T) -> T
where
    T: ops::Add<Output = T>,
{
    a + b
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sum() {
        assert_eq!(super::sum(1, 2), 3);
    }
}
