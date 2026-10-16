use std::fs;
use std::path::PathBuf;

fn source_root()->PathBuf{PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()}
fn source(path:&str)->String{fs::read_to_string(source_root().join(path)).expect("shipping source")}

#[test]
fn reference_docs_live_only_on_authorized_remote_runner_boundary(){
 let runner=source("box-exec-daemon/src/box_reference_docs_backend.rs");
 for n in ["provision_remote_reference_docs","atomic_write","box-doctor","request_box_help","Update Fabushi's Computer","legacy_reference_dir","workspace_root"]{assert!(runner.contains(n),"reference-doc Runner owner missing {n}");}
 assert!(!runner.contains("/home/box"));
 let lib=source("box-exec-daemon/src/lib.rs"); assert!(lib.contains("pub mod box_reference_docs_backend;"));
}

#[test]
fn window_owner_token_no_monitor_and_lease_are_remote_runner_semantics(){
 let runner=source("box-exec-daemon/src/box_windows_backend.rs");
 for n in ["/usr/local/bin/start-window","sand-start-window","SAND_BOX_WINDOW_UNAVAILABLE_EXIT_CODE","InvalidOwnerToken","NoMonitor","sand-monitor-busy-lease","touch /tmp/sand-monitor-busy-","SAND_BOX_WINDOW_OWNER_HEADER","SAND_BOX_DISPLAY_HEADER"]{assert!(runner.contains(n),"window Runner owner missing {n}");}
 assert!(runner.contains("SAND_BOX_PRIMARY_WINDOW_INDEX"));
}

#[test]
fn remote_log_acquisition_advances_only_delivered_offsets_and_recovers_checkpoint(){
 let runner=source("box-exec-daemon/src/box_log_shipper_backend.rs");
 for n in ["MAX_REMOTE_LOG_FILES","MAX_REMOTE_LOG_BYTES_PER_POLL","DeliverySettlement::Delivered","checkpoint","OffsetSnapshot","RemoteBoxLogShipper::open","symlink","sand-notify-injector-worker.log"]{assert!(runner.contains(n),"log shipper Runner owner missing {n}");}
 assert!(runner.contains("self.offsets.insert(key,end)"));
 assert!(!runner.contains("/home/box"));
}
