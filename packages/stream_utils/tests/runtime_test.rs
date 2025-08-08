use switchy_async::runtime::Handle;

#[switchy_async::test]
async fn runtime_test() {
    println!("Runtime test starting");

    let handle = Handle::current();
    println!("Got runtime handle");

    let join_handle = handle.spawn_with_name("test task", async {
        println!("Task is running");
        42
    });
    println!("Spawned task");

    let result = join_handle.await.unwrap();
    println!("Task completed with result: {result}");

    assert_eq!(result, 42);
}
