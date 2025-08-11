use moosicbox_web_server::{FromRequest, HttpRequest, Method, RequestData, Stub};

// Helper function to create a test HttpRequest with known data
fn create_test_request() -> HttpRequest {
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    let sim_req = SimulationRequest::new(Method::Get, "/test/path")
        .with_query_string("name=john&age=30&active=true")
        .with_header("user-agent", "test-agent")
        .with_header("content-type", "application/json")
        .with_header("authorization", "Bearer token123")
        .with_remote_addr("127.0.0.1:8080");

    HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)))
}

fn create_test_request_with_query(query: &str) -> HttpRequest {
    use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

    let sim_req = SimulationRequest::new(Method::Get, "/test").with_query_string(query);

    HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)))
}

fn test_request_data_sync_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing RequestData sync extraction...");

    let req = create_test_request();

    // Actually call from_request_sync
    let result = RequestData::from_request_sync(&req);

    match result {
        Ok(data) => {
            println!("✅ RequestData extracted successfully");
            println!("  Method: {:?}", data.method);
            println!("  Path: {}", data.path);
            println!("  Query: {}", data.query);
            println!("  Headers count: {}", data.headers.len());

            // Verify the extraction worked correctly
            assert_eq!(data.method, Method::Get);
            assert_eq!(data.path, "/test/path");
            assert_eq!(data.query, "name=john&age=30&active=true");
            assert!(data.user_agent.is_some());
            assert_eq!(data.user_agent.as_ref().unwrap(), "test-agent");
            assert!(data.content_type.is_some());
            assert_eq!(data.content_type.as_ref().unwrap(), "application/json");
            assert!(data.remote_addr.is_some());
            println!("✅ All RequestData fields extracted correctly");
        }
        Err(e) => {
            println!("❌ RequestData extraction failed: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

fn test_string_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing String extraction...");

    let req = create_test_request_with_query("hello world");

    let result = String::from_request_sync(&req);

    match result {
        Ok(value) => {
            println!("✅ String extracted: '{}'", value);
            assert_eq!(value, "hello world");
        }
        Err(e) => {
            println!("❌ String extraction failed: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

fn test_u32_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing u32 extraction...");

    // Test valid number
    let req = create_test_request_with_query("42");
    let result = u32::from_request_sync(&req);

    match result {
        Ok(value) => {
            println!("✅ u32 extracted: {}", value);
            assert_eq!(value, 42);
        }
        Err(e) => {
            println!("❌ u32 extraction failed: {}", e);
            return Err(e.into());
        }
    }

    // Test invalid number (should fail)
    let req = create_test_request_with_query("not_a_number");
    let result = u32::from_request_sync(&req);

    match result {
        Ok(_) => {
            println!("❌ u32 extraction should have failed for invalid input");
            return Err("Expected error for invalid u32".into());
        }
        Err(e) => {
            println!("✅ u32 extraction properly failed for invalid input: {}", e);
            assert!(e.to_string().contains("Failed to parse"));
        }
    }

    Ok(())
}

fn test_bool_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing bool extraction...");

    let test_cases = vec![
        ("true", true),
        ("1", true),
        ("yes", true),
        ("on", true),
        ("false", false),
        ("0", false),
        ("no", false),
        ("off", false),
        ("anything_else", false),
    ];

    for (input, expected) in test_cases {
        let req = create_test_request_with_query(input);
        let result = bool::from_request_sync(&req);

        match result {
            Ok(value) => {
                println!("✅ bool('{}') = {}", input, value);
                assert_eq!(value, expected, "Failed for input '{}'", input);
            }
            Err(e) => {
                println!("❌ bool extraction failed for '{}': {}", input, e);
                return Err(e.into());
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing synchronous extraction with FromRequest trait...");
    println!();

    // Run all the actual tests
    test_request_data_sync_extraction()?;
    println!();

    test_string_extraction()?;
    println!();

    test_u32_extraction()?;
    println!();

    test_bool_extraction()?;
    println!();

    println!("🎉 All synchronous FromRequest tests passed!");
    println!("📝 These tests actually validate extraction logic, not just imports");

    Ok(())
}
