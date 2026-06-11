use tempfile::NamedTempFile;
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_storage::write_traits::*;
use wasix_eth_storage::codecs::Table;
use wasix_eth_types::*;
use std::sync::{Arc, Barrier};
use std::thread;
use redb::ReadableDatabase;

#[test]
fn test_thread_visibility_failure_reproduction() {
    let tmp_file = NamedTempFile::new().unwrap();
    let db = EthDatabase::open(tmp_file.path()).unwrap();
    db.init_tables().unwrap();
    let db_arc = Arc::new(db);

    let num_readers = 5;
    let barrier = Arc::new(Barrier::new(num_readers + 1));
    let mut handles = Vec::new();

    // Spawn reader threads
    for i in 0..num_readers {
        let db_clone = db_arc.clone();
        let barrier_clone = barrier.clone();
        let handle = thread::spawn(move || {
            // Wait for writer to be ready
            barrier_clone.wait();
            
            // Wait for writer to finish writing
            barrier_clone.wait();
            
            // Start a long-running read transaction before the next write
            let tx = db_clone.inner().begin_read().unwrap();
            
            // Sync with main thread to allow it to write
            barrier_clone.wait();
            
            // Sync again after main thread writes
            barrier_clone.wait();

            // Try to read the data using the OLD transaction
            let table = tx.open_table(wasix_eth_storage::tables::Metadata::definition()).unwrap();
            let key = format!("thread_test_{}", i);
            let result = table.get(key.clone()).unwrap();
            (key, result.map(|v| v.value()))
        });
        handles.push(handle);
    }

    // Writer thread (main thread)
    let writer = DatabaseWriteProvider::new(db_arc.inner());
    
    // Sync with readers
    barrier.wait();
    
    // Write data for each reader
    for i in 0..num_readers {
        let key = format!("thread_test_{}", i);
        let value = Bytes::from(vec![i as u8; 10]);
        writer.set_metadata(key, value).unwrap();
    }
    
    // Signal readers that initial writing is done
    barrier.wait();

    // Signal readers that we are about to write more
    barrier.wait();

    // Write MORE data
    for i in 0..num_readers {
        let key = format!("thread_test_{}", i);
        let value = Bytes::from(vec![i as u8 + 10; 10]);
        writer.set_metadata(key, value).unwrap();
    }
    writer.commit().unwrap();

    // Signal readers that writing is done
    barrier.wait();

    // Collect results
    for handle in handles {
        let (key, result): (String, Option<Bytes>) = handle.join().unwrap();
        assert!(result.is_some(), "Reader failed to see data for key: {}", key);
        let value = result.unwrap();
        let i = key.split('_').last().unwrap().parse::<u8>().unwrap();
        // This confirms isolation: they see the data from the time they opened the transaction, 
        // NOT the data written after.
        assert_eq!(value, Bytes::from(vec![i; 10]), "Reader saw updated data instead of snapshot data for key: {}", key);
    }
}

#[test]
fn test_thread_visibility_simple() {
    let tmp_file = NamedTempFile::new().unwrap();
    let db = EthDatabase::open(tmp_file.path()).unwrap();
    db.init_tables().unwrap();
    let db_arc = Arc::new(db);

    let num_readers = 5;
    let barrier = Arc::new(Barrier::new(num_readers + 1));
    let mut handles = Vec::new();

    // Spawn reader threads
    for i in 0..num_readers {
        let db_clone = db_arc.clone();
        let barrier_clone = barrier.clone();
        let handle = thread::spawn(move || {
            let reader = DatabaseReadProvider::new(db_clone.inner());
            
            // Wait for writer to be ready
            barrier_clone.wait();
            
            // Wait for writer to finish writing
            barrier_clone.wait();
            
            // Try to read the data - this opens a NEW transaction per read
            let key = format!("thread_test_{}", i);
            let result = reader.get_metadata(key.clone()).unwrap();
            (key, result)
        });
        handles.push(handle);
    }

    // Writer thread (main thread)
    let writer = DatabaseWriteProvider::new(db_arc.inner());
    
    // Sync with readers
    barrier.wait();
    
    // Write data for each reader
    for i in 0..num_readers {
        let key = format!("thread_test_{}", i);
        let value = Bytes::from(vec![i as u8; 10]);
        writer.set_metadata(key, value).unwrap();
    }
    writer.commit().unwrap();
    
    // Signal readers that writing is done
    barrier.wait();

    // Collect results
    for handle in handles {
        let (key, result): (String, Option<Bytes>) = handle.join().unwrap();
        assert!(result.is_some(), "Reader failed to see data for key: {}", key);
        let value = result.unwrap();
        let i = key.split('_').last().unwrap().parse::<u8>().unwrap();
        assert_eq!(value, Bytes::from(vec![i; 10]), "Reader saw incorrect data for key: {}", key);
    }
}

#[test]
fn test_concurrent_writes_visibility() {
    let tmp_file = NamedTempFile::new().unwrap();
    let db = EthDatabase::open(tmp_file.path()).unwrap();
    db.init_tables().unwrap();
    let db_arc = Arc::new(db);

    let writer1 = DatabaseWriteProvider::new(db_arc.inner());
    let reader = DatabaseReadProvider::new(db_arc.inner());

    // Thread 1 writes something
    writer1.set_metadata("key1".to_string(), Bytes::from(vec![1])).unwrap();
    writer1.commit().unwrap();

    // Reader should see it
    assert!(reader.get_metadata("key1".to_string()).unwrap().is_some());

    // Thread 2 (new thread) should also see it
    let db_clone = db_arc.clone();
    thread::spawn(move || {
        let reader2 = DatabaseReadProvider::new(db_clone.inner());
        assert!(reader2.get_metadata("key1".to_string()).unwrap().is_some());
    }).join().unwrap();
}
