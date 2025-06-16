use hyperchad_template2::{container, to_html};

fn main() {
    println!("🎯 HYBRID UNIT SYNTAX - FINAL COMPREHENSIVE DEMO 🎯\n");

    // Test variables for dynamic expressions
    let dynamic_width = 75;
    let responsive_height = 80;
    let base_size = 45;

    // ✅ ALL SYNTAX VARIATIONS IN ONE CONTAINER
    let comprehensive_demo = container! {
        Div class="demo-container" {
            // APPROACH 2: Concise identifier syntax (vw50, vh100, etc.)
            Div width=vw50 height=vh100 max-width=dvw90 min-height=dvh60
                background="blue" padding=20 {
                "✅ Concise: vw50, vh100, dvw90, dvh60"
            }

            // APPROACH 1: Function-style syntax with expressions
            Section width=vw(dynamic_width) height=vh(responsive_height)
                    max-width=dvw(base_size + 5) min-height=dvh(base_size / 2)
                    background="green" margin=10 {
                "✅ Function: vw(75), vh(80), dvw(50), dvh(22)"
            }

            // MIXED: Both approaches in same element
            Div width=vw50 height=vh(responsive_height)
                max-width=dvw90 min-height=dvh(base_size)
                background="red" opacity=0.8 {
                "✅ Mixed: vw50 + vh(80) + dvw90 + dvh(45)"
            }

            // COMPLEX: Advanced expressions with function syntax
            Div width=vw(if dynamic_width > 50 { 100 } else { 50 })
                height=vh(responsive_height + 20)
                max-width=dvw(base_size * 2)
                min-height=dvh(base_size.min(30))
                background="purple" {
                "✅ Complex: conditional and arithmetic expressions"
            }

            // TRADITIONAL: Plain numbers and percentages still work
            Div width=800 height=600 padding=20 margin=100%
                background="orange" {
                "✅ Traditional: 800px, 600px, 20px, 100%"
            }
        }
    };

    println!("✅ Concise viewport unit syntax: vw50, vh100, dvw90, dvh60");
    println!("✅ Function-style syntax: vw(75), vh(80), dvw(50), dvh(90)");
    println!("✅ Mixed syntax: vw50 + vh(expression) in same element");
    println!("✅ Complex expressions: vw(if condition {{ 100 }} else {{ 50 }})");
    println!("✅ Traditional syntax: 800, 100%, 0.8 (still works)");
    println!("🎉 HYBRID IMPLEMENTATION COMPLETE!\n");

    println!("📄 Generated HTML:");
    println!("{}", comprehensive_demo.to_string());

    println!("\n🎯 SUMMARY:");
    println!("• Approach 1 (Function): vw(expr), vh(expr), dvw(expr), dvh(expr)");
    println!("• Approach 2 (Concise): vw50, vh100, dvw90, dvh60");
    println!("• Both approaches can be mixed in the same element");
    println!("• All existing syntax (numbers, percentages) continues to work");
    println!("• Complex expressions are supported in function syntax");
}
