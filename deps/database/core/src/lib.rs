use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::Connection;
use rusqlite::Error;
use rusqlite::Row;

pub trait Entryable: Sized {
    fn table_name() -> &'static str;
    fn p_key(&self) -> usize;
    fn p_key_column() -> &'static str { "id" }  // Default to "id"
    fn schema() -> &'static str;
    fn bind_values(&self) -> Vec<(&'static str, rusqlite::types::Value)>;
    fn from_row(row: &Row) -> Result<Self, Error>;
}

#[derive(Debug, Clone)]
pub struct DatabaseManager {
    connection: Arc<Mutex<Connection>>
}

impl DatabaseManager {
    pub fn new<T: Into<String>>(db_path: T) -> Result<Self, Error> {
        let connection = Connection::open(db_path.into())?;
        connection.execute("PRAGMA foreign_keys = ON", [])?;
        Ok(Self { connection: Arc::new(Mutex::new(connection)) })
    }

    // create table
    pub fn create_table<T: Entryable>(&self) -> Result<(), Error> {
        let connection = self.connection.lock().unwrap();
        let table_name = T::table_name();
        let schema = T::schema();
        let query = format!("CREATE TABLE IF NOT EXISTS {} ({})", table_name, schema);
        connection.execute(&query, [])?;
        Ok(())
    }

    // get entries
    // TODO: change this to use foregin keys and such
    pub fn get<T: Entryable>(&self, condition: Option<&str>) -> Result<Vec<T>, Error> {
    let connection = self.connection.lock().unwrap();
    let table_name = T::table_name();
    let query = match condition {
        Some(cond) => format!("SELECT * FROM {} WHERE {}", table_name, cond),
        None => format!("SELECT * FROM {}", table_name),
    };

    let mut stmt = connection.prepare(&query)?;

    let rows = stmt.query_map([], |row| {
        T::from_row(row)
    })?;

    let result: Result<Vec<T>, Error> = rows.collect();

    result
}

    // insert
    pub fn insert<T: Entryable>(&self, obj: T) -> Result<(), Error> {
        let connection = self.connection.lock().unwrap();
        let table_name = T::table_name();
        let bindings = obj.bind_values();
        let (names, values): (Vec<_>, Vec<_>) = bindings.into_iter().unzip();
        let placeholders: Vec<String> = (1..=values.len())
            .map(|i| format!("?{}", i))
            .collect();
        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            names.join(", "),
            placeholders.join(", ")
        );
        connection.execute(&query, rusqlite::params_from_iter(values))?;
        Ok(())
    }

    // upsert (insert or replace if exists)
    pub fn upsert<T: Entryable>(&self, obj: T) -> Result<T, Error> {
        let connection = self.connection.lock().unwrap();
        let table_name = T::table_name();
        // create table if not exists
        let schema = T::schema();
        connection.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({})", table_name, schema), [])?;
        let bindings = obj.bind_values();
        let (names, values): (Vec<_>, Vec<_>) = bindings.into_iter().unzip();
        let placeholders: Vec<String> = (1..=values.len())
            .map(|i| format!("?{}", i))
            .collect();
        let query = format!(
            "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
            table_name,
            names.join(", "),
            placeholders.join(", ")
        );
        connection.execute(&query, rusqlite::params_from_iter(values))?;
        Ok(obj)
    }

    // update
    pub fn update<T: Entryable>(&self, obj: T, condition: &str) -> Result<(), Error> {
        let connection = self.connection.lock().unwrap();
        let table_name = T::table_name();
        let bindings = obj.bind_values();
        let set_clauses: Vec<String> = bindings.iter()
            .map(|(name, _)| format!("{} = ?", name))
            .collect();
        let values: Vec<rusqlite::types::Value> = bindings.into_iter()
            .map(|(_, value)| value)
            .collect();
        let query = format!(
            "UPDATE {} SET {} WHERE {}",
            table_name,
            set_clauses.join(", "),
            condition
        );
        connection.execute(&query, rusqlite::params_from_iter(values))?;
        Ok(())
    }

    // delete
    pub fn delete<T: Entryable>(&self, obj: &T) -> Result<(), Error> {
        let connection = self.connection.lock().unwrap();
        let table_name = T::table_name();
        let condition = format!("{} = {}", T::p_key_column(), obj.p_key());
        let query = format!(
            "DELETE FROM {} WHERE {}",
            table_name,
            condition
        );
        connection.execute(&query, [])?;
        Ok(())
    }

    // clear all rows from table
    pub fn clear<T: Entryable>(&self) -> Result<(), Error> {
        let connection = self.connection.lock().unwrap();
        let table_name = T::table_name();
        connection.execute(&format!("DELETE FROM {}", table_name), [])?;
        Ok(())
    }

    // clear all rows, ignoring foreign key constraints
    pub fn clear_force<T: Entryable>(&self) -> Result<(), Error> {
        let connection = self.connection.lock().unwrap();
        let table_name = T::table_name();
        connection.execute("PRAGMA foreign_keys = OFF", [])?;
        let result = connection.execute(&format!("DELETE FROM {}", table_name), []);
        connection.execute("PRAGMA foreign_keys = ON", [])?;
        result?;
        Ok(())
    }
}
