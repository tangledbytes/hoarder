use zerocopy::FromBytes;

use crate::{
    error::HoarderError,
    mem::BufferHandle,
    protocol::{executor_protocol::*, network_protocol::MsgHeader},
};

pub struct ConnHandler {
    pub state: ConnHandlerState,
    socket: Fixed,
    buf_handle: Option<BufferHandle>,
    offset: u32,
}

#[derive(Debug)]
pub enum ConnHandlerState {
    Init,
    Recv(RecvPhase),
    Error(HoarderError),
}

#[derive(Debug)]
pub enum RecvPhase {
    ReceivingHeader,
    ReceivingBody,
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
            (MachineEvent::IoCompleted(_, _), ConnHandlerState::Error(_)) => todo!(),
            (_, _) => unreachable!(),
        };
        log::debug!("recv state - {:?}", self.state);
    }

    pub fn on_init(&mut self, ctx: &mut ExecutorContext) -> ConnHandlerState {
        match ctx.buffers.alloc() {
            Some(handle) => {
                self.buf_handle = Some(handle);
                self.recv(ctx);
                ConnHandlerState::Recv(RecvPhase::ReceivingHeader)
            }
            None => ConnHandlerState::Error(HoarderError::BufferAllocFail),
        }
    }

    pub fn on_recv(&mut self, ctx: &mut ExecutorContext, res: i32) -> ConnHandlerState {
        if res < 0 {
            log::error!("recv error: {res}");
        } else {
            self.offset += res as u32;
            log::debug!(
                "RECV - {:?}",
                MsgHeader::ref_from_bytes(ctx.buffers.get(self.buf_handle()).unwrap())
            )
        }

        // Submit a recv again
        self.recv(ctx);
        ConnHandlerState::Recv(RecvPhase::ReceivingHeader)
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
