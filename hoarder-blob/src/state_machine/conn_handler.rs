use hoarder_collections::alloc::BufferHandle;
use hoarder_common::error::{self, Errno, HoarderError};
use zerocopy::{FromBytes, KnownLayout};

use crate::protocol::{executor_protocol::*, network_protocol::MsgHeader};

pub struct ConnHandler {
    pub state: ConnHandlerState,
    socket: Fixed,
    buf_handle: Option<BufferHandle>,
    offset: u32,
}

#[derive(Debug, Copy, Clone)]
pub enum ConnHandlerState {
    Init,
    Recv(RecvPhase),
    Closing,
    Closed,
}

#[derive(Debug, Copy, Clone)]
pub enum RecvPhase {
    ReceivingHeader,
    ReceivingBody { header: MsgHeader },
    Processing,
}

impl ConnHandler {
    pub const fn new(socket_direct_descriptor: u32) -> Self {
        Self {
            state: ConnHandlerState::Init,
            socket: Fixed(socket_direct_descriptor),
            buf_handle: None,
            offset: 0,
        }
    }

    pub fn process_event(&mut self, ctx: &mut ExecutorContext, event: MachineEvent) {
        self.state = match (event, &self.state) {
            (MachineEvent::Spawn, ConnHandlerState::Init) => self.on_init(ctx),
            (MachineEvent::IoCompleted(res, _), ConnHandlerState::Recv(_)) => {
                self.on_recv(ctx, res)
            }
            (MachineEvent::IoCompleted(res, _), ConnHandlerState::Closing) => {
                self.on_closing(ctx, res)
            }
            (_, ConnHandlerState::Closed) => {
                hoarder_log::hdebug!("Received event while connection is closed");
                ConnHandlerState::Closed
            }
            (_, _) => unreachable!(),
        };
        hoarder_log::hdebug!("recv state - {:?}", self.state);
    }

    fn on_init(&mut self, ctx: &mut ExecutorContext) -> ConnHandlerState {
        match ctx.buffers.alloc() {
            Some(handle) => {
                self.buf_handle = Some(handle);
                self.recv(ctx);
                ConnHandlerState::Recv(RecvPhase::ReceivingHeader)
            }
            None => {
                ctx.submit_intent(MachineIntent::Retry);
                ConnHandlerState::Init
            }
        }
    }

    fn on_recv(&mut self, ctx: &mut ExecutorContext, res: i32) -> ConnHandlerState {
        if res == 0 {
            self.close(ctx);
            return ConnHandlerState::Closing;
        }

        let state = if res < 0 {
            self.handle_io_error(res, ctx)
        } else {
            self.offset += res as u32;
            let hdr_size = core::mem::size_of::<MsgHeader>();
            if self.offset as usize >= hdr_size {
                let hdr = MsgHeader::read_from_bytes(
                    &ctx.buffers.get(self.buf_handle()).unwrap()[..hdr_size],
                )
                .unwrap();
                ConnHandlerState::Recv(RecvPhase::ReceivingBody { header: hdr })
            } else {
                ConnHandlerState::Recv(RecvPhase::ReceivingHeader)
            }
        };

        self.recv(ctx);
        state
    }

    fn on_closing(&mut self, ctx: &mut ExecutorContext, res: i32) -> ConnHandlerState {
        if res == 0 {
            ctx.submit_intent(MachineIntent::Terminate);
            ConnHandlerState::Closed
        } else {
            // Try closing again
            self.close(ctx);
            ConnHandlerState::Closing
        }
    }

    fn handle_io_error(&mut self, err: i32, ctx: &mut ExecutorContext) -> ConnHandlerState {
        match Errno::from_raw_syscall_error(err) {
            Errno::EINTR | Errno::EAGAIN | Errno::ENOBUFS | Errno::ENOMEM | Errno::ETIMEDOUT => {
                self.recv(ctx);
                self.state
            }
            Errno::EBADF
            | Errno::EFAULT
            | Errno::EINVAL
            | Errno::ENOTSOCK
            | Errno::EAFNOSUPPORT
            | Errno::EOPNOTSUPP => {
                panic!("unexpected IO error encountered");
            }
            Errno::EIO
            | Errno::ECONNRESET
            | Errno::ENOTCONN
            | Errno::ESHUTDOWN
            | Errno::ECONNABORTED
            | Errno::ENETDOWN
            | Errno::ENETUNREACH
            | Errno::ENETRESET => {
                self.close(ctx);
                ConnHandlerState::Closing
            }
            _ => {
                hoarder_log::hwarn!("unexpected error err={}", err);
                // Just return the current state - no transitions
                self.state
            }
        }
    }

    fn close(&mut self, ctx: &mut ExecutorContext) {
        self.buf_handle
            .and_then(|handle| Some(ctx.buffers.free(handle)));
        self.offset = 0;
        ctx.submit_intent(MachineIntent::CloseFixed(self.socket));
    }

    fn buf_handle(&self) -> BufferHandle {
        // SAFETY: We acecss the handle only once we have initialized
        unsafe { self.buf_handle.unwrap_unchecked() }
    }

    fn recv(&mut self, ctx: &mut ExecutorContext) {
        ctx.submit_intent(MachineIntent::SubmitRecv(
            self.socket,
            self.buf_handle(),
            self.offset as _,
        ));
    }
}

#[cfg(test)]
mod test {
    extern crate std;
    use core::panic::AssertUnwindSafe;

    use hoarder_collections::alloc::BufferPool;
    use zerocopy::IntoBytes;

    use crate::{
        protocol::{
            executor_protocol::{ExecutorContext, MachineEvent, MachineIntent},
            network_protocol::MsgHeader,
        },
        state_machine::conn_handler::{ConnHandler, ConnHandlerState, RecvPhase},
    };

    #[cfg(test)]
    pub struct TestEnv {
        pub intents: [core::mem::MaybeUninit<MachineIntent>; 16],
        pub buf_pool: BufferPool<4096, 4096>,
    }

    #[cfg(test)]
    impl TestEnv {
        pub fn new() -> Self {
            Self {
                intents: [const { core::mem::MaybeUninit::uninit() }; 16],
                buf_pool: BufferPool::new(4),
            }
        }

        /// Creates an ExecutorContext that borrows from this environment
        pub fn context(&mut self) -> ExecutorContext<'_> {
            ExecutorContext {
                intents: &mut self.intents,
                len: 0,
                buffers: &mut self.buf_pool,
            }
        }
    }

    #[test]
    fn test_invalid_states() {
        let mut env = TestEnv::new();
        let ctx = &mut env.context();

        let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut ch = ConnHandler::new(1);
            ch.process_event(ctx, MachineEvent::Spawn);
            ch.process_event(ctx, MachineEvent::Spawn);
        }));

        assert!(res.is_err());
        ctx.len = 0;

        let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut ch = ConnHandler::new(1);
            ch.process_event(ctx, MachineEvent::IoCompleted(0, 0));
        }));

        assert!(res.is_err());
        ctx.len = 0;
    }

    #[test]
    fn test_valid_state_transitions() {
        let mut env = TestEnv::new();
        let ctx = &mut env.context();

        let mut ch = ConnHandler::new(1);

        ch.process_event(ctx, MachineEvent::Spawn);
        assert!(matches!(ch.state, ConnHandlerState::Recv(_)));
        ctx.len = 0;

        let test_data = "TEST".as_bytes();
        ctx.buffers.get_mut(ch.buf_handle()).unwrap()[..test_data.len()].copy_from_slice(test_data);
        ch.process_event(ctx, MachineEvent::IoCompleted(test_data.len() as i32, 0));
        assert!(matches!(ch.state, ConnHandlerState::Recv(_)));
        assert!(ctx.len == 1);
        ctx.len = 0;

        ch.process_event(ctx, MachineEvent::IoCompleted(0, 0));
        assert!(matches!(ch.state, ConnHandlerState::Closing));
        ctx.len = 0;

        ch.process_event(ctx, MachineEvent::IoCompleted(0, 0));
        assert!(matches!(ch.state, ConnHandlerState::Closed));
        ctx.len = 0;
    }

    #[test]
    fn test_should_parse_valid_msgs() {
        let mut env = TestEnv::new();
        let ctx = &mut env.context();

        let hdr = MsgHeader {
            magic: 0x1234,
            cmd: 1,
        };
        let bytes: &[u8] = hdr.as_bytes();

        let mut ch = ConnHandler::new(1);
        ch.process_event(ctx, MachineEvent::Spawn);
        assert!(matches!(ch.state, ConnHandlerState::Recv(_)));
        ctx.len = 0;

        ctx.buffers.get_mut(ch.buf_handle()).unwrap()[..5].copy_from_slice(bytes);
        ch.process_event(ctx, MachineEvent::IoCompleted(bytes.len() as i32, 0));
        match ch.state {
            ConnHandlerState::Recv(RecvPhase::ReceivingBody { header }) => {
                assert_eq!(header, hdr);
            }
            _ => assert!(false),
        }
        ctx.len = 0;
    }
}
