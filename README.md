# HOARDER
Hoarder is a educational project and is supposed to be apart of a bigger project. It is supposed to be read optimised BLOB storage exposing only 3 APIs:
1. `put_blob(key: u128, body: stream)` with support for maximum blob size to be `u32::MAX` ~ 4GB,
2. `get_blob(key: u128)`
3. `delete_blob(key: u128)`

The idea is to experiment with:
1. io_uring:
	1. Zero Copy I/O to disk.
	2. Zero Copy O to network (It needs NIC support, I don't have that).
	3. Experiments with single threaded event loop with multi-threaded IO. I am not intuitively convinced that such setup can exhaust
	bigger machines.
2. Testing:
	1. Fuzzing
	2. Determinisic Simulation Testing