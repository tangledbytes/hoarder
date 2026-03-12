use core::mem::MaybeUninit;

use crate::{
    error::{HoarderError, Result},
    io::IO,
    mem::{
        BufferPool, ObjectHandle, ObjectPool,
        collections::{Array, RingBuffer},
    },
    protocol::executor_protocol::{ExecutorContext, MachineEvent, MachineIntent},
    state_machine::{conn_handler::ConnHandler, tcp_server::TcpServer},
};

const TCP_SERVER: u8 = 1;
const CONN_HANDLER: u8 = 2;

pub struct Executor<Io: IO> {
    io: Io,
    tcp_listener_pool: ObjectPool<TcpServer, TCP_SERVER>,
    conn_handler_pool: ObjectPool<ConnHandler, CONN_HANDLER>,
    io_bufs: BufferPool,
    run_queues: [RingBuffer<RunQueueEvent>; 2],
    intents: [MaybeUninit<MachineIntent>; 16],
    current_run_queue: usize,
}

impl<Io> Executor<Io>
where
    Io: IO,
{
    pub fn new(io: Io, listeners: u32, conn_handlers: u32, buf_count: u32) -> Self {
        let run_queues_size = conn_handlers as usize * 2;

        let tcp_listener_pool = ObjectPool::new(listeners);
        let conn_handler_pool = ObjectPool::new(conn_handlers);
        let io_bufs = BufferPool::new(buf_count);
        let run_queues = [
            RingBuffer::new(run_queues_size),
            RingBuffer::new(run_queues_size),
        ];

        Self {
            io,
            tcp_listener_pool,
            conn_handler_pool,
            io_bufs,
            run_queues,
            current_run_queue: 0,
            intents: [const { MaybeUninit::uninit() }; 16],
        }
    }

    pub fn run(&mut self) {
        self.setup().unwrap();
        self.seed_run_queue();

        loop {
            // Drain the current queue
            while let Some(RunQueueEvent { reason, handle }) =
                self.run_queues[self.current_run_queue].pop()
            {
                let intents = match handle.pool_id() {
                    TCP_SERVER => match self.tcp_listener_pool.get_mut(handle) {
                        Some(listener) => {
                            log::debug!("TCP_SERVER {reason:?} {handle:?}",);
                            let mut ctx = ExecutorContext {
                                len: 0,
                                intents: &mut self.intents,
                                buffers: &mut self.io_bufs,
                            };
                            listener.process_event(&mut ctx, reason);
                            ctx.len
                        }
                        None => {
                            log::warn!("TCP_SERVER MISSING {reason:?} {handle:?}");
                            0
                        }
                    },
                    CONN_HANDLER => match self.conn_handler_pool.get_mut(handle) {
                        Some(conn_handler) => {
                            log::debug!("CONN_HANDLER {reason:?} {handle:?}",);
                            let mut ctx = ExecutorContext {
                                len: 0,
                                intents: &mut self.intents,
                                buffers: &mut self.io_bufs,
                            };

                            conn_handler.process_event(&mut ctx, reason);
                            ctx.len
                        }
                        None => {
                            log::warn!("CONN_HANDLER MISSING {reason:?} {handle:?}");
                            0
                        }
                    },
                    _ => unreachable!(),
                };

                // Consume the intents
                self.complete_intent(intents, handle);
            }

            // Submit IO
            self.io.submit_and_wait(1).unwrap();

            // Drain the completion queue
            let next_run_queue_idx = self.next_run_queue_idx();
            while let Some(cqe) = self.io.completion().next() {
                self.run_queues[next_run_queue_idx]
                    .push(RunQueueEvent {
                        reason: MachineEvent::IoCompleted(cqe.result(), cqe.flags()),
                        handle: cqe.user_data().try_into().unwrap(),
                    })
                    .unwrap();
            }

            // Switch run queue
            self.switch_run_queue();
        }
    }

    fn complete_intent(&mut self, intents: usize, handle: ObjectHandle) {
        for intent in &self.intents[..intents] {
            match unsafe { intent.assume_init_ref() } {
                MachineIntent::Retry => {
                    self.run_queues[self.next_run_queue_idx()]
                        .push(RunQueueEvent {
                            reason: MachineEvent::Spawn,
                            handle,
                        })
                        .unwrap();
                }
                MachineIntent::SubmitMultishotAccept(fd) => {
                    let entry = io_uring::opcode::AcceptMulti::new(*fd)
                        .allocate_file_index(true)
                        .build()
                        .user_data(handle.into());
                    self.io.enqueue(&entry).unwrap();
                }
                MachineIntent::SpawnConnHandler(fixed) => {
                    let handle = self
                        .conn_handler_pool
                        .spawn(ConnHandler::new(*&fixed.0))
                        .unwrap();
                    self.run_queues[self.current_run_queue]
                        .push(RunQueueEvent {
                            reason: MachineEvent::Spawn,
                            handle,
                        })
                        .unwrap();
                }
                MachineIntent::SubmitRecv(fixed, buffer_id, offset) => {
                    let offset = *offset as usize;
                    let buf_size = self.io_bufs.pool().buf_size;

                    assert!(offset < buf_size);
                    let buf = self.io_bufs.get_mut(*buffer_id).unwrap()[offset..].as_mut_ptr();
                    let remaining_len = (buf_size - offset) as u32;

                    let entry = io_uring::opcode::ReadFixed::new(
                        *fixed,
                        buf,
                        remaining_len,
                        buffer_id.0.index as _,
                    )
                    .build()
                    .user_data(handle.into());
                    self.io.enqueue(&entry).unwrap();
                }
                MachineIntent::Terminate => {
                    match handle.pool_id() {
                        TCP_SERVER => self.tcp_listener_pool.despawn(handle),
                        CONN_HANDLER => self.conn_handler_pool.despawn(handle),
                        _ => unreachable!(),
                    };
                }
            }
        }
    }

    fn switch_run_queue(&mut self) {
        self.current_run_queue = 1 - self.current_run_queue;
    }

    fn next_run_queue_idx(&self) -> usize {
        1 - self.current_run_queue
    }

    fn seed_run_queue(&mut self) {
        let handle = self
            .tcp_listener_pool
            .spawn(TcpServer::new("0.0.0.0:3000", 4096))
            .unwrap();
        self.run_queues[self.current_run_queue]
            .push(RunQueueEvent {
                handle,
                reason: MachineEvent::Spawn,
            })
            .unwrap();
    }

    fn setup(&mut self) -> Result<()> {
        self.io
            .register_files(self.conn_handler_pool.capacity() as _)?;

        let sz = self.io_bufs.pool().count;
        let bufs = Array::from_fixed_iter(
            sz,
            (0..sz).map(|idx| libc::iovec {
                iov_base: self.io_bufs.pool().buf_ptr(idx) as *mut libc::c_void,
                iov_len: 0x1000,
            }),
        );
        unsafe { self.io.register_buffers(&bufs) }
    }
}

#[derive(Debug)]
struct RunQueueEvent {
    reason: MachineEvent,
    handle: ObjectHandle,
}
