use switchy_async::sync::mpsc::unbounded;

#[test_log::test(switchy_async::test)]
async fn simple_channel_test() {
    println!("Creating channel");
    let (sender, receiver) = unbounded();

    println!("Sending data");
    sender.send(42).unwrap();

    println!("Receiving data");
    let value = receiver.recv().unwrap();
    println!("Received: {value}");

    assert_eq!(value, 42);
}
