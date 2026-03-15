# TODO
1. Document all the unsafe code.
2. Improve the IO module.
3. Improve the UringIO implementation. The IO submission code is extremely naive.
	1. Should have support for flushing submission queue if full instead of returning error.
	2. Should have support for reaping completion queue if full instead of returning error on submission.
	3. Should not wait for unlimited time in `submit_and_wait`. Submit a timeout op as well for timing bound
	on the function.
4. ✅ Narrow down allocations. Allocation should not happen anywhere other than the executor and that too shouldn't
use allocation directly instead it should import everything from the `crate::mem`. 
5. ✅ No use of Vec for VecDequeue or any such structures which can cause dynamic allocations. Replace them with
heap allocated arrays.
6. Create a semi-simulated executor which just simulates networking for now. For the implementation of
disk state machine.
7. ✅ Clean up unused code.
8. Add unit tests for all the modules.
9. ✅ Clean up logging and figure out a better way to log.
	1. ✅ Need to figure out how to log without dyanmic allocation.
	2. ✅ Need to figure out **when** to log. Definitely cannot log on hot paths.
10. Need to do better error handling. Need to differentiate between logical errors and client errors. Most logical
errors should do a clean exit and client errors should be propagated to the top properly. Right now everything just
crashes the system, which isn't ideal.
11. Logger writes only to stdout regardless of log level. Might want to add support for writing to stderr.
12. Logger should ignore some log levels when logging in `--release` mode.

# Bugs
1. In src/state_machine/tcp_server.rs, when setting up the listening socket, you convert the safe rustix::fd::OwnedFd into a raw file descriptor:
self.listener_fd = fd.into_raw_fd();

However, TcpServer does not implement Drop. When a TcpServer instance is removed from the ObjectPool or the program errors out, this listener socket file descriptor will leak. You should either wrap this in a safe type that implements Drop (like standardizing on OwnedFd where possible) or implement Drop for TcpServer to explicitly close the raw fd.
