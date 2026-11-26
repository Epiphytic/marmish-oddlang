use marmish_engine::{InputReceipt, MaintenanceReport};

fn main() {
    let initial = MaintenanceReport::default();
    let _: (u32, u64) = (initial.records_removed, initial.spool_bytes_removed);
    assert_eq!((initial.records_removed, initial.spool_bytes_removed), (0, 0));
    println!("{initial:#?}");
    for (records_removed, spool_bytes_removed) in [(0, 0), (1, u64::MAX), (u32::MAX, 1), (u32::MAX, u64::MAX)] {
        let mut report = MaintenanceReport { records_removed, spool_bytes_removed };
        let snapshot = report.clone();
        report.records_removed = report.records_removed.wrapping_add(1);
        assert_eq!((snapshot.records_removed, snapshot.spool_bytes_removed), (records_removed, spool_bytes_removed));
        println!("{snapshot:?}");
    }
    for (accepted, available_credit, duplicate) in [(0u64, 0u32, false), (u64::MAX, u32::MAX, true),
                                                     (1, u32::MAX, false), (u64::MAX, 1, true)] {
        let mut receipt = InputReceipt { accepted, available_credit, duplicate };
        let snapshot = receipt.clone();
        let _: (u64, u32, bool) = (snapshot.accepted, snapshot.available_credit, snapshot.duplicate);
        receipt.duplicate = !receipt.duplicate;
        assert_ne!(snapshot.duplicate, receipt.duplicate);
        assert_eq!((snapshot.accepted, snapshot.available_credit, snapshot.duplicate),
                   (accepted, available_credit, duplicate));
        println!("{snapshot:?}");
    }
}
