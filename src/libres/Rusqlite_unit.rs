#![allow(non_camel_case_types)]

pub use rusqlite::backup::Backup as AC_Backup;
pub use rusqlite::params as AC_params;
pub use rusqlite::{
    Connection as AC_Connection, Error as AC_Error, OpenFlags as AC_OpenFlags,
    OptionalExtension as AC_OptionalExtension, Row as AC_Row, Transaction as AC_Transaction,
    TransactionBehavior as AC_TransactionBehavior,
};

pub type AC_Result<T> = rusqlite::Result<T>;
