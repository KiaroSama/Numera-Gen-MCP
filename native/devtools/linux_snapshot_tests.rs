use super::*;

fn record(name: &[u8]) -> Vec<u8> {
    let mut bytes = b"123 (".to_vec();
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(b") S 100 123 123 0 0 0 1 2 3 4 11 12 0 0 20 0 3 0 99 4096 42\n");
    bytes
}
#[test]
fn numeric_stat_parser_ignores_arbitrary_comm_bytes_and_keeps_documented_field_indexes() {
    for name in [b"name with ) (spaces".as_slice(), b"private-marker\n\xff"] {
        let value = stat(&record(name)).unwrap();
        assert_eq!(value.identity.pid, 123);
        assert_eq!(value.identity.ppid, 100);
        assert_eq!(value.identity.group, 123);
        assert_eq!(value.identity.start, 99);
        assert_eq!(value.row.user_ticks, 11);
        assert_eq!(value.row.system_ticks, 12);
        assert_eq!(value.row.threads, 3);
        assert_eq!(value.row.rss_pages, 42);
        assert_eq!(value.row.major_faults, 3);
        let output = serde_json::to_string(&value.row).unwrap();
        assert!(!output.contains("private-marker"));
    }
    for bytes in [b"123 (x) S 1".as_slice(), b"123 x S 1", b"-1 (x) S 1"] {
        assert!(stat(bytes).is_err());
    }
    let overflow = record(b"x");
    let text = String::from_utf8(overflow)
        .unwrap()
        .replace(" 99 ", " 18446744073709551616 ");
    assert!(stat(text.as_bytes()).is_err());
    for (state, expected) in [
        ("R", 1),
        ("S", 2),
        ("D", 3),
        ("Z", 4),
        ("T", 5),
        ("t", 6),
        ("X", 7),
        ("x", 7),
        ("I", 8),
        ("P", 9),
    ] {
        let bytes = String::from_utf8(record(b"states"))
            .unwrap()
            .replace(") S ", &format!(") {state} "));
        assert_eq!(stat(bytes.as_bytes()).unwrap().row.state, expected);
    }
    let original = String::from_utf8(record(b"fields")).unwrap();
    let (prefix, fields) = original.split_once(") ").unwrap();
    let fields = fields.split_ascii_whitespace().collect::<Vec<_>>();
    for at in [1, 2, 9, 11, 12, 17, 19, 21] {
        for bad in ["-1", "1.5", "18446744073709551616"] {
            let mut changed = fields.clone();
            changed[at] = bad;
            assert!(stat(format!("{prefix}) {}", changed.join(" ")).as_bytes()).is_err());
        }
    }
    for state in ["?", "SS"] {
        assert!(stat(original.replace(") S ", &format!(") {state} ")).as_bytes()).is_err());
    }
    for (at, bad) in [(1, "4294967296"), (2, "2147483648")] {
        let mut changed = fields.clone();
        changed[at] = bad;
        assert!(stat(format!("{prefix}) {}", changed.join(" ")).as_bytes()).is_err());
    }
    assert!(stat(original.replacen("123 (", "4294967296 (", 1).as_bytes()).is_err());
}
#[test]
fn io_and_wait_projection_never_returns_raw_private_or_unknown_kernel_text() {
    assert_eq!(
        io_counts(b"rchar: 10\nread_bytes: 20\nwrite_bytes: 30\n").unwrap(),
        [10, 20, 30]
    );
    for value in [
        b"rchar: -1\nread_bytes: 0\nwrite_bytes: 0".as_slice(),
        b"rchar: 1\nrchar: 2\nread_bytes: 3\nwrite_bytes: 4",
        b"rchar: 1",
    ] {
        assert!(io_counts(value).is_err());
    }
    assert_eq!(wait_class(b"0\n"), 0);
    assert_eq!(wait_class(b"do_wait"), 1);
    assert_eq!(wait_class(b"futex_wait_queue"), 2);
    assert_eq!(wait_class(b"io_schedule"), 3);
    assert_eq!(wait_class(b"private-marker /secret/path"), 4);
    for bytes in [b"kernel_wait4".as_slice(), b"__do_wait"] {
        assert_eq!(wait_class(bytes), 1);
    }
    for bytes in [b"futex_wait_queue_me".as_slice(), b"futex_wait"] {
        assert_eq!(wait_class(bytes), 2);
    }
    for bytes in [
        b"folio_wait_bit_common".as_slice(),
        b"wait_on_page_bit_common",
    ] {
        assert_eq!(wait_class(bytes), 3);
    }
    assert_eq!(wait_class(b""), 0);
    for bytes in [
        b"rchar: 1\nread_bytes: 2\n".as_slice(),
        b"read_bytes: 2\nwrite_bytes: 3",
        b"rchar: 1\nwrite_bytes: 2",
    ] {
        assert!(io_counts(bytes).is_err());
    }
    assert_eq!(
        io_counts(b"ignored\nunknown: private\nrchar: 1\nread_bytes: 2\nwrite_bytes: 3").unwrap(),
        [1, 2, 3]
    );
}
#[test]
fn held_proc_identity_rejects_pid_start_parent_or_group_changes() {
    let mut process = Process::open(std::process::id()).unwrap();
    let original = process.identity;
    assert!(process.current().is_ok());
    assert!(process.executable().unwrap().1 > 0);
    assert!(Process::open(0).is_err());
    assert!(relative(&process.directory, "bad\0name", false).is_err());
    assert!(relative(&process.directory, "exe", false).is_err());
    assert!(bounded(relative(&process.directory, "stat", false).unwrap(), 1).is_err());
    for changed in [
        Identity {
            pid: original.pid + 1,
            ..original
        },
        Identity {
            ppid: original.ppid + 1,
            ..original
        },
        Identity {
            group: original.group + 1,
            ..original
        },
        Identity {
            start: original.start + 1,
            ..original
        },
    ] {
        process.identity = changed;
        assert!(process.current().is_err());
    }
    process.identity = original;
    assert!(process.current().is_ok());
}
