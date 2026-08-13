#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum PublicError {
    Usage,
    CodeViolation,
    ServiceUnavailable,
    Internal,
    Temporary,
    Protocol,
    Authorization,
    Configuration,
    ToolchainNotExecutable,
    ToolchainNotFound,
}

impl PublicError {
    pub(crate) fn exit_code(self) -> i32 {
        match self {
            Self::Usage => 64,
            Self::CodeViolation => 65,
            Self::ServiceUnavailable => 69,
            Self::Internal => 70,
            Self::Temporary => 75,
            Self::Protocol => 76,
            Self::Authorization => 77,
            Self::Configuration => 78,
            Self::ToolchainNotExecutable => 126,
            Self::ToolchainNotFound => 127,
        }
    }

    pub(crate) fn public_message(self) -> &'static str {
        match self {
            Self::Usage => {
                "antdelirium[AD-USAGE-0001]: invocação inválida; consulte a ajuda disponível para esta sessão"
            }
            Self::CodeViolation => {
                "antdelirium[AD-CODE-0001]: governança bloqueou a compilação; leia guard/governance_error.json"
            }
            Self::ServiceUnavailable => {
                "antdelirium[AD-SERVICE-0001]: serviço de governança indisponível; tente novamente"
            }
            Self::Internal => {
                "antdelirium[AD-INTERNAL-0001]: falha interna de governança; informe a referência opaca"
            }
            Self::Temporary => {
                "antdelirium[AD-TEMP-0001]: governança temporariamente ocupada; tente novamente"
            }
            Self::Protocol => {
                "antdelirium[AD-PROTOCOL-0001]: cliente e serviço de governança são incompatíveis; solicite o operador"
            }
            Self::Authorization => {
                "antdelirium[AD-AUTH-0001]: operação não autorizada para esta sessão"
            }
            Self::Configuration => {
                "antdelirium[AD-CONFIG-0001]: configuração de governança inválida; não altere arquivos externos ao projeto; solicite o operador"
            }
            Self::ToolchainNotExecutable => {
                "antdelirium[AD-TOOLCHAIN-0001]: não foi possível executar o compilador configurado"
            }
            Self::ToolchainNotFound => {
                "antdelirium[AD-TOOLCHAIN-0002]: compilador configurado não foi encontrado"
            }
        }
    }
}

pub(crate) struct OutputRouter;

impl OutputRouter {
    pub(crate) fn write_success(output: &str) {
        print!("{output}");
        if !output.ends_with('\n') {
            println!();
        }
    }

    pub(crate) fn exit(error: PublicError) -> ! {
        eprintln!("{}", error.public_message());
        std::process::exit(error.exit_code());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_error_codes_have_the_normative_exit_mapping() {
        let cases = [
            (PublicError::Usage, "AD-USAGE-0001", 64),
            (PublicError::CodeViolation, "AD-CODE-0001", 65),
            (PublicError::ServiceUnavailable, "AD-SERVICE-0001", 69),
            (PublicError::Internal, "AD-INTERNAL-0001", 70),
            (PublicError::Temporary, "AD-TEMP-0001", 75),
            (PublicError::Protocol, "AD-PROTOCOL-0001", 76),
            (PublicError::Authorization, "AD-AUTH-0001", 77),
            (PublicError::Configuration, "AD-CONFIG-0001", 78),
            (
                PublicError::ToolchainNotExecutable,
                "AD-TOOLCHAIN-0001",
                126,
            ),
            (PublicError::ToolchainNotFound, "AD-TOOLCHAIN-0002", 127),
        ];
        for (error, code, exit) in cases {
            assert_eq!(error.exit_code(), exit);
            assert!(error.public_message().contains(code));
        }
    }

    #[test]
    fn public_errors_never_expose_private_diagnostics() {
        for error in [
            PublicError::Usage,
            PublicError::ServiceUnavailable,
            PublicError::Internal,
            PublicError::Protocol,
            PublicError::Authorization,
            PublicError::Configuration,
        ] {
            let message = error.public_message();
            for forbidden in [
                "/home/",
                "/var/",
                "state.db",
                "SELECT ",
                "backtrace",
                "plugins/",
                "plan/",
                "active_plan",
            ] {
                assert!(!message.contains(forbidden));
            }
        }
    }
}
