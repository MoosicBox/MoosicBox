use moosicbox_web_server::{FromRequest, HttpRequest, Method, RequestData, Stub};

// Helper function to create a test HttpRequest with known data
fn create_test_request() -> HttpRequest {
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    let sim_req = SimulationRequest::new(Method::Post, "/api/users")
        .with_query_string("filter=active&limit=10")
        .with_header("user-agent", "async-test-client")
        .with_header("content-type", "application/json")
        .with_header("accept", "application/json")
        .with_remote_addr("192.168.1.100:3000");

    HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)))
}

fn create_test_request_with_query(query: &str) -> HttpRequest {
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    let sim_req = SimulationRequest::new(Method::Get, "/async/test").with_query_string(query);

    HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)))
}

async fn test_request_data_async_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing RequestData async extraction...");

    let req = create_test_request();

    // Actually call from_request_async and await the result
    let result = RequestData::from_request_async(req).await;

    match result {
        Ok(data) => {
            println!("✅ RequestData extracted asynchronously");
            println!("  Method: {:?}", data.method);
            println!("  Path: {}", data.path);
            println!("  Query: {}", data.query);
            println!("  Headers count: {}", data.headers.len());

            // Verify the extraction worked correctly
            assert_eq!(data.method, Method::Post);
            assert_eq!(data.path, "/api/users");
            assert_eq!(data.query, "filter=active&limit=10");
            assert!(data.user_agent.is_some());
            assert_eq!(data.user_agent.as_ref().unwrap(), "async-test-client");
            assert!(data.content_type.is_some());
            assert_eq!(data.content_type.as_ref().unwrap(), "application/json");
            assert!(data.remote_addr.is_some());
            println!("✅ All RequestData fields extracted correctly via async");
        }
        Err(e) => {
            println!("❌ RequestData async extraction failed: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

async fn test_async_vs_sync_consistency() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing async vs sync extraction consistency...");

    // Create identical requests for both tests
    let req_for_sync = create_test_request_with_query("consistency_test=123");
    let req_for_async = create_test_request_with_query("consistency_test=123");

    // Extract using sync method
    let sync_result = String::from_request_sync(&req_for_sync);

    // Extract using async method
    let async_result = String::from_request_async(req_for_async).await;

    match (sync_result, async_result) {
        (Ok(sync_value), Ok(async_value)) => {
            println!("✅ Sync result: '{}'", sync_value);
            println!("✅ Async result: '{}'", async_value);

            // Verify they produce identical results
            assert_eq!(
                sync_value, async_value,
                "Sync and async extraction should produce identical results"
            );
            println!("✅ Sync and async extraction produce identical results");
        }
        (Err(sync_err), Err(async_err)) => {
            println!("✅ Both sync and async failed consistently");
            println!("  Sync error: {}", sync_err);
            println!("  Async error: {}", async_err);
            // Both failing is also consistent behavior
        }
        (sync_result, async_result) => {
            println!("❌ Inconsistent results between sync and async:");
            println!("  Sync: {:?}", sync_result);
            println!("  Async: {:?}", async_result);
            return Err("Sync and async extraction produced different results".into());
        }
    }

    Ok(())
}

async fn test_async_i32_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing i32 async extraction...");

    // Test valid negative number
    let req = create_test_request_with_query("-42");
    let result = i32::from_request_async(req).await;

    match result {
        Ok(value) => {
            println!("✅ i32 extracted asynchronously: {}", value);
            assert_eq!(value, -42);
        }
        Err(e) => {
            println!("❌ i32 async extraction failed: {}", e);
            return Err(e.into());
        }
    }

    // Test invalid input
    let req = create_test_request_with_query("not_an_integer");
    let result = i32::from_request_async(req).await;

    match result {
        Ok(_) => {
            println!("❌ i32 async extraction should have failed for invalid input");
            return Err("Expected error for invalid i32".into());
        }
        Err(e) => {
            println!(
                "✅ i32 async extraction properly failed for invalid input: {}",
                e
            );
            assert!(e.to_string().contains("Failed to parse"));
        }
    }

    Ok(())
}

async fn test_future_types() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Future types are properly implemented...");

    let req = create_test_request_with_query("future_test");

    // Test that we can actually await the futures
    let string_future = String::from_request_async(req);
    let string_result = string_future.await;

    match string_result {
        Ok(value) => {
            println!("✅ Future<String> resolved correctly: '{}'", value);
            assert_eq!(value, "future_test");
        }
        Err(e) => {
            println!("❌ Future<String> failed: {}", e);
            return Err(e.into());
        }
    }

    // Test RequestData future
    let req2 = create_test_request();
    let data_future = RequestData::from_request_async(req2);
    let data_result = data_future.await;

    match data_result {
        Ok(data) => {
            println!("✅ Future<RequestData> resolved correctly");
            println!("  Method: {:?}", data.method);
        }
        Err(e) => {
            println!("❌ Future<RequestData> failed: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing asynchronous extraction with FromRequest trait...");
    println!();

    // Run all the actual async tests
    test_request_data_async_extraction().await?;
    println!();

    test_async_vs_sync_consistency().await?;
    println!();

    test_async_i32_extraction().await?;
    println!();

    test_future_types().await?;
    println!();

    println!("🎉 All asynchronous FromRequest tests passed!");
    println!("📝 These tests validate actual async extraction logic and Future types");

    Ok(())
}
