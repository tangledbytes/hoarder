pub(crate) mod executor_protocol {
    use core::mem::MaybeUninit;

    use hoarder_collections::alloc::{BufferHandle, BufferPool};

    pub type IoResult = i32;
    pub type IoFlag = u32;
    pub use hoarder_io::io_uring::{
        cqueue::more as more_io,
        types::{Fd, Fixed},
    };

    #[derive(Debug)]
    pub enum MachineEvent {
        Spawn,
        IoCompleted(IoResult, IoFlag),
    }

    pub enum MachineIntent {
        /// Retry simply asks the executor to reattempt
        /// the same operation on the state machine on
        /// the next tick.
        Retry,

        /// Terminate the machine should put the machine
        /// back into the machine pool for reuse
        Terminate,

        /// CloseFixed should take a Fixed index and should
        /// close it
        CloseFixed(Fixed),

        /// SpawnConnHandler will attempt to spawn
        /// a connection handler which will listen
        /// on the given DIRECT_DESCRIPTOR_INDEX
        SpawnConnHandler(Fixed),

        /// MultishotAccept will setup a AcceptMulti
        /// io with the given Fd as the listener socket
        SubmitMultishotAccept(Fd),

        /// Recv is similar to recv(2) and accepts
        /// a [Fixed] file, a [BufferId] (which can)
        /// be obtained from the executor's buffer pool
        /// and an offset into the buffer.
        SubmitRecv(Fixed, BufferHandle, u16),
    }

    pub struct ExecutorContext<'a> {
        pub intents: &'a mut [MaybeUninit<MachineIntent>],
        pub len: usize,

        pub buffers: &'a mut BufferPool,
    }

    impl<'a> ExecutorContext<'a> {
        pub fn submit_intent(&mut self, intent: MachineIntent) {
            assert!(self.len < self.intents.len());
            self.intents[self.len].write(intent);
            self.len += 1;
        }
    }
}

pub mod network_protocol {
    use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unalign};

    #[derive(Debug, FromBytes, KnownLayout, Immutable, IntoBytes, PartialEq, Copy, Clone)]
    #[repr(packed)]
    pub struct MsgHeader {
        pub magic: u32,
        pub cmd: u8,
    }

    #[derive(Debug, FromBytes)]
    pub struct MsgHeaderLong {
        pub header: MsgHeader,
        pub key: u128,
        pub crc: u32,
        pub len: u32,
    }
}
