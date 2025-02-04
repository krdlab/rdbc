//! The RDBC (Rust DataBase Connectivity) API is loosely based on the ODBC and JDBC standards
//! and provides a database agnostic programming interface for executing queries and fetching
//! results.
//!
//! Reference implementation RDBC Drivers exist for Postgres, MySQL and SQLite.
//!
//! The following example demonstrates how RDBC can be used to run a trivial query against Postgres.
//!
//! ```rust,ignore
//! use rdbc::*;
//! use rdbc_postgres::PostgresDriver;
//!
//! let driver = PostgresDriver::new();
//! let mut conn = driver.connect("postgres://postgres:password@localhost:5433").unwrap();
//! let mut stmt = conn.prepare("SELECT a FROM b WHERE c = ?").unwrap();
//! let mut rs = stmt.execute_query(&[Value::Int32(123)]).unwrap();
//! while rs.next().unwrap() {
//!   println!("{:?}", rs.get_string(1));
//! }
//! ```

/// RDBC Error
#[derive(Debug)]
pub enum Error {
    General(String),
}

#[derive(Debug, Clone)]
pub enum Value {
    Int32(i32),
    UInt32(u32),
    String(String),
    //TODO add other types
}

impl ToString for Value {
    fn to_string(&self) -> String {
        match self {
            Value::Int32(n) => format!("{}", n),
            Value::UInt32(n) => format!("{}", n),
            Value::String(s) => format!("'{}'", s),
        }
    }
}

/// RDBC Result type
pub type Result<T> = std::result::Result<T, Error>;

/// Represents database driver that can be shared between threads, and can therefore implement
/// a connection pool
pub trait Driver: Sync + Send {
    /// Create a connection to the database. Note that connections are intended to be used
    /// in a single thread since most database connections are not thread-safe
    fn connect(&self, url: &str) -> Result<Box<dyn Connection>>;
}

/// Represents a connection to a database
pub trait Connection {
    /// Create a statement for execution
    fn create_statement(&mut self) -> Result<Box<dyn Statement + '_>>;

    /// Create a prepared statement for execution
    fn prepare_statement(&mut self, sql: &str) -> Result<Box<dyn PreparedStatement + '_>>;

    fn commit(&mut self) -> Result<()>;

    fn rollback(&mut self) -> Result<()>;

    fn close(self) -> Result<()>;
}

/// Represents an executable statement
pub trait Statement {
    /// Execute a query that is expected to return a result set, such as a `SELECT` statement
    fn execute_query(&mut self, sql: &str, params: &[Value]) -> Result<Box<dyn ResultSet + '_>>;

    /// Execute a query that is expected to update some rows.
    fn execute_update(&mut self, sql: &str, params: &[Value]) -> Result<u64>;

    fn close(self) -> Result<()>;
}

pub trait PreparedStatement {
    fn execute_query(&mut self, params: &[Value]) -> Result<Box<dyn ResultSet + '_>>;

    fn execute_update(&mut self, params: &[Value]) -> Result<u64>;

    fn close(self) -> Result<()>;
}

/// Result set from executing a query against a statement
pub trait ResultSet {
    /// get meta data about this result set
    fn meta_data(&self) -> Result<Box<dyn ResultSetMetaData>>;

    /// Move the cursor to the next available row if one exists and return true if it does
    fn next(&mut self) -> Result<bool>;

    fn get_i8(&mut self, i: u64) -> Result<Option<i8>>;
    fn get_i16(&mut self, i: u64) -> Result<Option<i16>>;
    fn get_i32(&mut self, i: u64) -> Result<Option<i32>>;
    fn get_i64(&mut self, i: u64) -> Result<Option<i64>>;
    fn get_f32(&mut self, i: u64) -> Result<Option<f32>>;
    fn get_f64(&mut self, i: u64) -> Result<Option<f64>>;
    fn get_string(&mut self, i: u64) -> Result<Option<String>>;
    fn get_bytes(&mut self, i: u64) -> Result<Option<Vec<u8>>>;

    fn close(self) -> Result<()>;
}

/// Meta data for result set
pub trait ResultSetMetaData {
    fn num_columns(&self) -> u64;
    fn column_name(&self, i: u64) -> String;
    fn column_type(&self, i: u64) -> DataType;
    fn column_display_size(&self, i: u64) -> u64;
}

/// RDBC Data Types
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DataType {
    Bool,
    Byte,
    Char,
    Short,
    Integer,
    Float,
    Double,
    Decimal,
    Date,
    Time,
    Datetime,
    Utf8,
    Binary,
}

#[derive(Debug, Clone)]
pub struct Column {
    name: String,
    data_type: DataType,
    display_size: u64,
}

impl Column {
    pub fn new(name: &str, data_type: DataType, display_size: u64) -> Self {
        Column {
            name: name.to_owned(),
            data_type,
            display_size,
        }
    }
}

impl ResultSetMetaData for Vec<Column> {
    fn num_columns(&self) -> u64 {
        self.len() as u64
    }

    fn column_name(&self, i: u64) -> String {
        self[i as usize].name.clone()
    }

    fn column_type(&self, i: u64) -> DataType {
        self[i as usize].data_type
    }

    fn column_display_size(&self, i: u64) -> u64 {
        self[i as usize].display_size
    }
}
