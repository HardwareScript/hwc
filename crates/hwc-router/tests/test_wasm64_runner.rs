use hwc_router::ffi::{HwcRoutingTask64, Wasm64RouterRunner};
use std::thread;

#[test]
fn test_wasm64_runner_global_dispatch() {
    let runner = Wasm64RouterRunner::default();
    let payload = vec![1, 2, 3, 4, 5];

    let result = runner.invoke_global_plugin(&payload);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), payload);

    let empty_payload: Vec<u8> = vec![];
    assert!(runner.invoke_global_plugin(&empty_payload).is_err());
}

#[test]
fn test_wasm64_runner_thread_local_isolation() {
    let mut handles = Vec::new();
    for thread_id in 0..8 {
        let payload = vec![thread_id as u8; 16];
        let handle = thread::spawn(move || {
            let thread_runner = Wasm64RouterRunner::default();
            let res = thread_runner.invoke_detailed_plugin_on_thread(&payload);
            assert!(res.is_ok());
            let out = res.unwrap();
            assert_eq!(out.len(), 16);
            assert_eq!(out[0], thread_id as u8);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Worker thread panicked");
    }
}

#[test]
fn test_wasm64_runner_execute_abi() {
    let runner = Wasm64RouterRunner::default();
    let dummy_data = [0u8; 8];
    let task = HwcRoutingTask64 {
        num_nets: 0,
        num_obstacles: 0,
        num_access_points: 0,
        task_payload_ptr: dummy_data.as_ptr(),
        task_payload_len: dummy_data.len() as u64,
    };

    let result = runner.execute(&task);
    assert!(result.is_ok());
    let output = result.unwrap();
    assert_eq!(output.status_code, 0);
    assert_eq!(output.wire_count, 0);
}
