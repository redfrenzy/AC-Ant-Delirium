fn command_contract() -> &'static str {
    "t76-standard-commands"
}

fn main() {
    println!("{}", command_contract());
}

#[cfg(test)]
mod tests {
    use super::command_contract;

    #[test]
    fn reports_the_standard_command_contract() {
        assert_eq!(command_contract(), "t76-standard-commands");
    }
}
