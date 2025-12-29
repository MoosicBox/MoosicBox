use switchy_async::sync::mpsc;

#[switchy_async::test]
async fn compare_channel_behavior() {
    println!("=== Testing Flume Channel ===");
    let (flume_tx, flume_rx) = flume::bounded::<i32>(16);

    // Test immediate drop
    {
        let (tx, rx) = flume::bounded::<i32>(16);
        println!("Flume: Created and dropping immediately");
        drop(tx);
        drop(rx);
        println!("Flume: Dropped successfully");
    }

    // Test send after drop
    drop(flume_rx);
    match flume_tx.try_send(42) {
        Ok(_) => println!("Flume: Send succeeded after receiver drop"),
        Err(e) => println!("Flume: Send failed after receiver drop: {e:?}"),
    }

    println!("=== Testing Switchy Channel ===");
    // FIXME: Enable when bounded channel is implemented
    // let (switchy_tx, switchy_rx) = mpsc::bounded::<i32>(16);
    let (switchy_tx, switchy_rx) = mpsc::unbounded::<i32>();

    // Test immediate drop
    {
        // FIXME: Enable when bounded channel is implemented
        // let (tx, rx) = switchy_mpsc::bounded::<i32>(16);
        let (tx, rx) = mpsc::unbounded::<i32>();
        println!("Switchy: Created and dropping immediately");
        drop(tx);
        drop(rx);
        println!("Switchy: Dropped successfully");
    }

    // Test send after drop
    drop(switchy_rx);
    match switchy_tx.send(42) {
        Ok(_) => println!("Switchy: Send succeeded after receiver drop"),
        Err(e) => println!("Switchy: Send failed after receiver drop: {e:?}"),
    }
}
