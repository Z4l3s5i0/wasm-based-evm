May 06 19:57:09.025 INFO  [Storage] Adding block #4 with hash 0x5bede739fed44b0b29ac46435b9d1d2c2e283aabe3a14e455eaa726c8b83af3a

thread 'tokio-runtime-worker' panicked at src\storage\storage.rs:479:28:
called `Result::unwrap()` on an `Err` value: Storage(Io(Os { code: 29, kind: Uncategorized, message: "I/O error" }))
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

















May 06 19:56:41.148 ERROR [Sync] Failed to download block 1: error sending request for url (http://192.168.1.152:9002/): error trying to connect: tcp connect error: Bad file descriptor (os error 8)
May 06 19:56:41.150 ERROR [Sync] Sync step failed: error sending request for url (http://192.168.1.152:9002/): error trying to connect: tcp connect error: Bad file descriptor (os error 8)
May 06 19:56:51.161 INFO  [Executor] Executing 0 transactions for block 1
May 06 19:56:51.163 INFO  [Storage] Adding block #1 with hash 0x1a4ca22fb0b2b4fe19b948d10f3229142032442a0d7f881eba47cdb39e812fc5

thread 'tokio-runtime-worker' panicked at src\storage\storage.rs:358:51:
called `Result::unwrap()` on an `Err` value: Storage(PreviousIo)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace