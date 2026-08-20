fn main() {
    println!("authoritative-ci-fixture");
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixture_is_compilable() {
        assert_eq!(2 + 2, 4);
    }
}
