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
}
#[test]
fn held_proc_identity_rejects_pid_start_parent_or_group_changes() {
    let mut process = Process::open(std::process::id()).unwrap();
    let original = process.identity;
    assert!(process.current().is_ok());
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
