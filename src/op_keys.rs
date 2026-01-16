//! Operation keys mapping for Lustre job stats

use std::collections::HashMap;
use std::sync::LazyLock;

/// Short name to long name mapping for operations
pub static OP_KEYS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("ops", "ops");
    m.insert("cr", "create");
    m.insert("op", "open");
    m.insert("cl", "close");
    m.insert("mn", "mknod");
    m.insert("ln", "link");
    m.insert("ul", "unlink");
    m.insert("mk", "mkdir");
    m.insert("rm", "rmdir");
    m.insert("mv", "rename");
    m.insert("ga", "getattr");
    m.insert("sa", "setattr");
    m.insert("gx", "getxattr");
    m.insert("sx", "setxattr");
    m.insert("st", "statfs");
    m.insert("sy", "sync");
    m.insert("rd", "read");
    m.insert("wr", "write");
    m.insert("pu", "punch");
    m.insert("mi", "migrate");
    m.insert("fa", "fallocate");
    m.insert("dt", "destroy");
    m.insert("gi", "get_info");
    m.insert("si", "set_info");
    m.insert("qc", "quotactl");
    m.insert("pa", "prealloc");
    m.insert("rb", "read_bytes");
    m.insert("wb", "write_bytes");
    m
});

/// Long name to short name mapping for operations (reverse of OP_KEYS)
pub static OP_KEYS_REV: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("ops", "ops");
    m.insert("create", "cr");
    m.insert("open", "op");
    m.insert("close", "cl");
    m.insert("mknod", "mn");
    m.insert("link", "ln");
    m.insert("unlink", "ul");
    m.insert("mkdir", "mk");
    m.insert("rmdir", "rm");
    m.insert("rename", "mv");
    m.insert("getattr", "ga");
    m.insert("setattr", "sa");
    m.insert("getxattr", "gx");
    m.insert("setxattr", "sx");
    m.insert("statfs", "st");
    m.insert("sync", "sy");
    m.insert("read", "rd");
    m.insert("write", "wr");
    m.insert("punch", "pu");
    m.insert("migrate", "mi");
    m.insert("fallocate", "fa");
    m.insert("destroy", "dt");
    m.insert("get_info", "gi");
    m.insert("set_info", "si");
    m.insert("quotactl", "qc");
    m.insert("prealloc", "pa");
    m.insert("read_bytes", "rb");
    m.insert("write_bytes", "wb");
    m
});

/// Misc keys for timestamps
pub static MISC_KEYS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("sw", "sampling_window");
    m.insert("sn", "snapshot_time");
    m.insert("ts", "timestamp");
    m
});

/// Job ID name keys for parsing job IDs
pub static JOBID_NAME_KEYS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("%e", "exe");
    m.insert("%g", "group");
    m.insert("%u", "user");
    m.insert("%p", "proc");
    m.insert("%j", "job");
    m.insert("%H", "host_short");
    m.insert("%h", "host");
    m
});

/// Get the ordered list of operation short keys (for consistent iteration)
#[allow(dead_code)]
pub fn op_short_keys() -> Vec<&'static str> {
    vec![
        "ops", "cr", "op", "cl", "mn", "ln", "ul", "mk", "rm", "mv",
        "ga", "sa", "gx", "sx", "st", "sy", "rd", "wr", "pu", "mi",
        "fa", "dt", "gi", "si", "qc", "pa", "rb", "wb",
    ]
}

/// Get the ordered list of operation long keys
#[allow(dead_code)]
pub fn op_long_keys() -> Vec<&'static str> {
    vec![
        "ops", "create", "open", "close", "mknod", "link", "unlink", "mkdir",
        "rmdir", "rename", "getattr", "setattr", "getxattr", "setxattr", "statfs",
        "sync", "read", "write", "punch", "migrate", "fallocate", "destroy",
        "get_info", "set_info", "quotactl", "prealloc", "read_bytes", "write_bytes",
    ]
}

/// Check if a key is an operation key (long name)
pub fn is_op_key(key: &str) -> bool {
    OP_KEYS_REV.contains_key(key)
}

