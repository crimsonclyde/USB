use super::*;
use serde_json::{Value, json};

fn disk() -> Value {
    json!({"kname":"sdb","path":"/dev/sdb","type":"disk","maj:min":"8:16","ro":false,
        "tran":"usb","fstype":null,"fsver":null,"mountpoints":[null],"size":1000000,"pkname":null})
}
fn partition() -> Value {
    json!({"kname":"sdb1","path":"/dev/sdb1","type":"part","maj:min":"8:17","ro":false,
        "tran":null,"fstype":"vfat","fsver":"FAT32","mountpoints":[null],"size":900000,"pkname":"sdb"})
}
fn parse(blocks: Vec<Value>) -> Result<Inventory> {
    parse_snapshot(&serde_json::to_vec(&json!({"blockdevices":blocks})).unwrap())
}

#[test]
fn usb_single_partition_and_whole_disk_filesystems_are_review_candidates_only() {
    let inventory = parse(vec![partition(), disk()]).unwrap();
    assert!(!inventory.live_snapshot);
    assert_eq!(inventory.devices[0].block.kname, "sdb");
    assert!(!inventory.devices[0].candidate_for_review);
    assert!(inventory.devices[1].candidate_for_review);
    assert_eq!(inventory.devices[1].filesystem, Some(Filesystem::Fat32));
    for fs in ["exfat", "ext4"] {
        let mut block = disk();
        block["fstype"] = fs.into();
        assert!(parse(vec![block]).unwrap().devices[0].candidate_for_review);
    }
}

#[test]
fn every_missing_or_unsafe_fact_rejects_without_using_removable_flags() {
    for (field, value, reason) in [
        ("ro", json!(true), Rejection::ReadOnlyOrUnknown),
        ("ro", Value::Null, Rejection::ReadOnlyOrUnknown),
        ("size", json!(0), Rejection::EmptyOrUnknownSize),
        ("size", Value::Null, Rejection::EmptyOrUnknownSize),
        ("size", json!(1000001), Rejection::EmptyOrUnknownSize),
        ("fstype", json!("ntfs"), Rejection::UnsupportedFilesystem),
        ("fstype", Value::Null, Rejection::UnsupportedFilesystem),
        ("fsver", json!("FAT16"), Rejection::UnsupportedFilesystem),
        ("fsver", Value::Null, Rejection::UnsupportedFilesystem),
        (
            "mountpoints",
            json!(["/media/visitor"]),
            Rejection::MountedDisk,
        ),
    ] {
        let mut block = partition();
        block[field] = value;
        let inventory = parse(vec![disk(), block]).unwrap();
        assert!(!inventory.devices[1].candidate_for_review, "{field}");
        assert!(inventory.devices[1].rejections.contains(&reason));
    }
    for transport in [Value::Null, json!("sata"), json!("nvme")] {
        let mut block = disk();
        block["tran"] = transport;
        assert!(
            parse(vec![block, partition()]).unwrap().devices[1]
                .rejections
                .contains(&Rejection::NotUsb)
        );
    }
    let mut block = disk();
    block["mountpoints"] = json!(["/"]);
    assert!(
        parse(vec![block, partition()])
            .unwrap()
            .devices
            .iter()
            .all(|d| !d.candidate_for_review)
    );
}

#[test]
fn multiple_partitions_and_device_mapper_trees_are_rejected() {
    let mut other = partition();
    other["kname"] = "sdb2".into();
    other["path"] = "/dev/sdb2".into();
    other["maj:min"] = "8:18".into();
    assert!(
        parse(vec![disk(), partition(), other])
            .unwrap()
            .devices
            .iter()
            .all(|d| !d.candidate_for_review)
    );
    let mut mapper = partition();
    mapper["type"] = "crypt".into();
    assert!(
        parse(vec![disk(), mapper])
            .unwrap()
            .devices
            .iter()
            .all(|d| !d.candidate_for_review)
    );
    // An unsupported host mapper must not hide an independent USB candidate.
    let mut host = partition();
    host["kname"] = "dm-0".into();
    host["path"] = "/dev/mapper/host-root".into();
    host["type"] = "crypt".into();
    host["maj:min"] = "253:0".into();
    host["pkname"] = Value::Null;
    host["mountpoints"] = json!(["/"]);
    let inventory = parse(vec![host, disk(), partition()]).unwrap();
    assert!(!inventory.devices[0].candidate_for_review);
    assert!(inventory.devices[2].candidate_for_review);
}

#[test]
fn identities_unknown_metadata_parent_cycles_and_integer_overflow_fail_closed() {
    for (field, value) in [
        ("kname", json!("../sdb1")),
        ("path", json!("/etc/passwd")),
        ("maj:min", json!("4294967296:1")),
        ("maj:min", json!("8:01")),
        ("size", json!(-1)),
        ("ro", json!(0)),
        ("extra", json!(true)),
        ("pkname", json!("missing")),
        ("pkname", json!("sdb1")),
        ("mountpoints", json!(["relative"])),
    ] {
        let mut block = partition();
        block[field] = value;
        assert!(parse(vec![disk(), block]).is_err(), "{field}");
    }
    assert!(parse(vec![disk(), partition(), partition()]).is_err());
    let mut duplicate = partition();
    duplicate["maj:min"] = "8:16".into();
    assert!(parse(vec![disk(), duplicate]).is_err());
    let mut parent = disk();
    parent["pkname"] = "sdb1".into();
    assert!(parse(vec![parent, partition()]).is_err());
    assert!(parse_snapshot(br#"{"blockdevices":[],"extra":true}"#).is_err());
    assert!(parse_snapshot(br#"{"blockdevices":[],"blockdevices":[]}"#).is_err());
    assert!(parse_snapshot(&vec![b' '; MAX_SNAPSHOT_BYTES + 1]).is_err());
    assert!(parse_snapshot(br#"{"blockdevices":[{"size":18446744073709551616}]}"#).is_err());
}

#[test]
fn inventory_order_is_reproducible_and_empty_snapshots_are_valid() {
    let a = parse(vec![disk(), partition()]).unwrap();
    let b = parse(vec![partition(), disk()]).unwrap();
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
    assert!(parse(vec![]).unwrap().devices.is_empty());
    assert!(parse(vec![disk(); MAX_DEVICES + 1]).is_err());
}

#[test]
fn collector_handles_missing_failed_and_oversized_helpers_without_fallbacks() {
    assert!(matches!(
        collect(Command::new("/not-a-bureau-test-command")),
        Err(Error::Io(_))
    ));
    assert!(matches!(
        collect(Command::new("/usr/bin/false")),
        Err(Error::Invalid(_))
    ));
    let fixture = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(fixture.path(), vec![b' '; MAX_SNAPSHOT_BYTES + 1]).unwrap();
    let mut command = Command::new("/usr/bin/cat");
    command.arg(fixture.path());
    assert!(matches!(collect(command), Err(Error::Invalid(_))));
}

#[test]
fn collector_parses_generated_helper_output_and_reports_its_provenance() {
    let fixture = tempfile::NamedTempFile::new().unwrap();
    let bytes = serde_json::to_vec(&json!({"blockdevices":[disk(), partition()]})).unwrap();
    std::fs::write(fixture.path(), &bytes).unwrap();
    let mut command = Command::new("/usr/bin/cat");
    command.arg(fixture.path());
    let inventory = collect(command).unwrap();
    assert!(inventory.live_snapshot);
    assert!(inventory.devices[1].candidate_for_review);
    assert_eq!(std::fs::read(fixture.path()).unwrap(), bytes);
}
