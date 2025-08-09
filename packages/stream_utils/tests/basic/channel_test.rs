use switchy_async::sync::mpsc::unbounded;

#[test_log::test(switchy_async::test)]
async fn channel_test() {
    println!("Testing channel");

    let (sender, mut receiver) = unbounded();

    // Send data in background task
    let sender_clone = sender.clone();
    switchy_async::task::spawn(async move {
        println!("Sending data");
        sender_clone.send(42).unwrap();
        println!("Data sent");
    });

    println!("Receiving data");
    let value = receiver.recv_async().await.unwrap();
    println!("Received: {value}");

    assert_eq!(value, 42);
}
