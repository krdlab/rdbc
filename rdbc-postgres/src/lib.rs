//! Postgres RDBC Driver
//!
//! This crate implements an RDBC Driver for the `postgres` crate.
//!
//! The RDBC (Rust DataBase Connectivity) API is loosely based on the ODBC and JDBC standards.
//!
//! ```rust,no_run
//! use rdbc::*;
//! use rdbc_postgres::PostgresDriver;
//!
//! let driver = PostgresDriver::new();
//! let mut conn = driver.connect("postgres://postgres:password@localhost:5433").unwrap();
//! let mut stmt = conn.prepare("SELECT a FROM b WHERE c = ?").unwrap();
//! let mut rs = stmt.execute_query(&[Value::Int32(123)]).unwrap();
//! while rs.next() {
//!   println!("{:?}", rs.get_string(1));
//! }
//! ```

use postgres::row::Row;
use postgres::{Client, NoTls};

use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::keywords::Keyword::NoKeyword;
use sqlparser::tokenizer::{Token, Tokenizer, Word};

use postgres::types::Type;

pub struct PostgresDriver {}

impl PostgresDriver {
    pub fn new() -> Self {
        PostgresDriver {}
    }
}

impl rdbc::Driver for PostgresDriver {
    fn connect(&self, url: &str) -> rdbc::Result<Box<dyn rdbc::Connection>> {
        let c = postgres::Client::connect(url, NoTls).map_err(to_rdbc_err)?;
        Ok(Box::new(PConnection::new(c)))
    }
}

struct PConnection {
    conn: Client,
}

impl PConnection {
    pub fn new(conn: Client) -> Self {
        Self { conn }
    }
}

impl rdbc::Connection for PConnection {
    fn create(&mut self, sql: &str) -> rdbc::Result<Box<dyn rdbc::Statement + '_>> {
        self.prepare(sql)
    }

    fn prepare(&mut self, sql: &str) -> rdbc::Result<Box<dyn rdbc::Statement + '_>> {
        // translate SQL, mapping ? into $1 style bound param placeholder
        let dialect = PostgreSqlDialect {};
        let mut tokenizer = Tokenizer::new(&dialect, sql);
        let tokens = tokenizer.tokenize().unwrap();
        let mut i = 0;
        let tokens: Vec<Token> = tokens
            .iter()
            .map(|t| match t {
                Token::Placeholder(c) if *c == "?" => {
                    i += 1;
                    Token::Word(Word {
                        value: format!("${}", i),
                        quote_style: None,
                        keyword: NoKeyword,
                    })
                }
                _ => t.clone(),
            })
            .collect();
        let sql = tokens
            .iter()
            .map(|t| format!("{}", t))
            .collect::<Vec<String>>()
            .join("");

        Ok(Box::new(PStatement {
            conn: &mut self.conn,
            sql,
        }))
    }

    fn commit(&mut self) -> rdbc::Result<()> {
        let _ = self.conn.query(r"COMMIT", &[]).map_err(to_rdbc_err)?;
        Ok(())
    }

    fn rollback(&mut self) -> rdbc::Result<()> {
        let _ = self.conn.query(r"ROLLBACK", &[]).map_err(to_rdbc_err)?;
        Ok(())
    }

    fn close(self) -> rdbc::Result<()> {
        self.conn.close().map_err(to_rdbc_err)
    }
}

struct PStatement<'a> {
    conn: &'a mut Client,
    sql: String,
}

impl<'a> rdbc::Statement for PStatement<'a> {
    fn execute_query(
        &mut self,
        params: &[rdbc::Value],
    ) -> rdbc::Result<Box<dyn rdbc::ResultSet + '_>> {
        let params = to_postgres_value(params);
        let params: Vec<_> = params.iter().map(|v| v.as_ref()).collect();

        let rows = self
            .conn
            .query(&self.sql, params.as_slice())
            .map_err(to_rdbc_err)?;
        let meta = rows
            .first()
            .map(|r| {
                r.columns()
                    .iter()
                    .map(|c| {
                        rdbc::Column::new(
                            c.name(),
                            to_rdbc_type(c.type_()),
                            to_rdbc_display_size(c.type_()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Box::new(PResultSet { meta, i: 0, rows }))
    }

    fn execute_update(&mut self, params: &[rdbc::Value]) -> rdbc::Result<u64> {
        let params = to_postgres_value(params);
        let params: Vec<_> = params.iter().map(|v| v.as_ref()).collect();
        self.conn
            .execute(&self.sql, params.as_slice())
            .map_err(to_rdbc_err)
    }

    fn close(self) -> rdbc::Result<()> {
        Ok(())
    }
}

struct PResultSet {
    meta: Vec<rdbc::Column>,
    i: usize,
    rows: Vec<Row>,
}

macro_rules! impl_resultset_fns {
    ($($fn: ident -> $ty: ty),*) => {
        $(
            fn $fn(&self, i: u64) -> rdbc::Result<Option<$ty>> {
                Ok(self.rows.get(self.i - 1).map(|r| r.get(i as usize)))
            }
        )*
    }
}

impl rdbc::ResultSet for PResultSet {
    fn meta_data(&self) -> rdbc::Result<Box<dyn rdbc::ResultSetMetaData>> {
        Ok(Box::new(self.meta.clone()))
    }

    fn next(&mut self) -> bool {
        if self.i < self.rows.len() {
            self.i = self.i + 1;
            true
        } else {
            false
        }
    }

    impl_resultset_fns! {
        get_i8 -> i8,
        get_i16 -> i16,
        get_i32 -> i32,
        get_i64 -> i64,
        get_f32 -> f32,
        get_f64 -> f64,
        get_string -> String,
        get_bytes -> Vec<u8>
    }

    fn close(self) -> rdbc::Result<()> {
        Ok(())
    }
}

/// Convert a Postgres error into an RDBC error
fn to_rdbc_err(e: postgres::error::Error) -> rdbc::Error {
    rdbc::Error::General(format!("{:?}", e))
}

fn to_rdbc_type(ty: &Type) -> rdbc::DataType {
    match ty.name() {
        "" => rdbc::DataType::Bool,
        //TODO all types
        _ => rdbc::DataType::Utf8,
    }
}

// ref: https://github.com/pgjdbc/pgjdbc/blob/258cff34e32f8b5fe5d71f7553f7ac5f30690f85/pgjdbc/src/main/java/org/postgresql/jdbc/TypeInfoCache.java#L899
fn to_rdbc_display_size(ty: &Type) -> u64 {
    if ty.oid() == Type::INT4.oid() {
        11
    } else {
        10 // TODO
    }
}

fn to_postgres_value(values: &[rdbc::Value]) -> Vec<Box<dyn postgres::types::ToSql + Sync>> {
    values
        .iter()
        .map(|v| match v {
            rdbc::Value::String(s) => Box::new(s.clone()) as Box<dyn postgres::types::ToSql + Sync>,
            rdbc::Value::Int32(n) => Box::new(*n) as Box<dyn postgres::types::ToSql + Sync>,
            rdbc::Value::UInt32(n) => Box::new(*n) as Box<dyn postgres::types::ToSql + Sync>,
            //TODO all types
        })
        .collect()
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::sync::Arc;

    #[test]
    fn execute_query() -> rdbc::Result<()> {
        execute("DROP TABLE IF EXISTS test", &vec![])?;
        execute("CREATE TABLE test (a INT NOT NULL)", &vec![])?;
        execute(
            "INSERT INTO test (a) VALUES (?)",
            &vec![rdbc::Value::Int32(123)],
        )?;

        let driver: Arc<dyn rdbc::Driver> = Arc::new(PostgresDriver::new());
        let mut conn = driver.connect("postgres://rdbc:secret@127.0.0.1:5433")?;
        let mut stmt = conn.prepare("SELECT a FROM test")?;
        let mut rs = stmt.execute_query(&vec![])?;

        assert!(rs.next());
        assert_eq!(Some(123), rs.get_i32(0)?);
        assert!(!rs.next());

        Ok(())
    }

    fn execute(sql: &str, values: &Vec<rdbc::Value>) -> rdbc::Result<u64> {
        println!("Executing '{}' with {} params", sql, values.len());
        let driver: Arc<dyn rdbc::Driver> = Arc::new(PostgresDriver::new());
        let mut conn = driver.connect("postgres://rdbc:secret@127.0.0.1:5433")?;
        let mut stmt = conn.prepare(sql)?;
        stmt.execute_update(values)
    }
}
