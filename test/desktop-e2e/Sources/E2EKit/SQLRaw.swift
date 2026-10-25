import Foundation
import SQLite3

/// 场景自带的夹具表（如幕僚活动）用：在被测 App 停止时写入，或只读查一个整数。
public enum SQLRaw {
    public static func execute(database url: URL, statements: [(sql: String, bindings: [String])]) throws {
        let database = try SQLiteDB(path: url.path, write: true)
        defer { database.close() }
        for statement in statements {
            try database.execute(statement.sql, bindings: statement.bindings)
        }
    }

    /// 只读取第一行第一列的整数；没有行时为 nil。
    public static func scalar(database url: URL, _ sql: String) throws -> Int? {
        let database = try SQLiteDB(path: url.path, write: false)
        defer { database.close() }
        return try database.query(sql) { Int(sqlite3_column_int64($0, 0)) }.first
    }
}
