use core::str::FromStr;
use hoarder_common::error::{HoarderError, Result};
use rustix::{fd::IntoRawFd, net::*};

use crate::protocol::executor_protocol::*;

pub struct TcpServer {
    pub state: TcpServerState,
    addr: &'static str,
    backlog: u16,
    listener_fd: i32,
}

#[derive(Debug)]
pub enum TcpServerState {
    Init,
    Accepting,
    Error(HoarderError),
}

impl TcpServer {
    pub const fn new(addr: &'static str, backlog: u16) -> Self {
        Self {
            state: TcpServerState::Init,
            addr,
            backlog,
            listener_fd: -1,
        }
    }

    pub fn process_event(&mut self, ctx: &mut ExecutorContext, event: MachineEvent) {
        match event {
            MachineEvent::Spawn => match &self.state {
                TcpServerState::Init => {
                    match self.listen().and_then(|_| Ok(self.multishot_accept(ctx))) {
                        Ok(_) => self.state = TcpServerState::Accepting,
                        Err(e) => self.state = TcpServerState::Error(e),
                    }
                }
                _ => unreachable!(),
            },
            MachineEvent::IoCompleted(res, flags) => match &self.state {
                TcpServerState::Accepting => {
                    if res < 0 {
                        self.state = TcpServerState::Error(res.into());
                        return;
                    }

                    let direct_descriptor_idx = Fixed(res as u32);
                    ctx.submit_intent(MachineIntent::SpawnConnHandler(direct_descriptor_idx));

                    // If not accepting anymore - Rearm with multishot_accept again
                    if !more_io(flags) {
                        self.multishot_accept(ctx);
                    }
                }
                TcpServerState::Error(_) => { /* Do nothing for now */ }
                _ => unreachable!(),
            },
        }
    }

    fn listen(&mut self) -> Result<()> {
        let parsed_addr = SocketAddr::from_str(self.addr).unwrap();
        let domain = match parsed_addr {
            SocketAddr::V4(_) => AddressFamily::INET,
            SocketAddr::V6(_) => AddressFamily::INET6,
        };

        let fd = socket(domain, SocketType::STREAM, None)
            .map_err(|e| HoarderError::IoError(e.raw_os_error()))?;
        sockopt::set_socket_reuseaddr(&fd, true)
            .map_err(|e| HoarderError::IoError(e.raw_os_error()))?;
        bind(&fd, &SocketAddrAny::from(parsed_addr))
            .map_err(|e| HoarderError::IoError(e.raw_os_error()))?;
        listen(&fd, self.backlog as i32).map_err(|e| HoarderError::IoError(e.raw_os_error()))?;

        self.listener_fd = fd.into_raw_fd();
        Ok(())
    }

    fn multishot_accept(&self, ctx: &mut ExecutorContext) {
        ctx.submit_intent(MachineIntent::SubmitMultishotAccept(Fd(self.listener_fd)));
    }
}
