use std::sync::mpsc::channel;
use std::thread;

pub(crate) fn new_thread_schedule<
  T: Send + Sync + 'static,
  R: Send + Sync + 'static,
>(
  task: impl FnOnce(Option<T>) -> R + Send + 'static,
  state: Option<T>,
) -> R {
  let (sender, receiver) = channel();
  thread::spawn(move || {
    sender
      .send(task(state))
      .expect("new thread send message failed")
  });
  receiver.recv().unwrap()
}
