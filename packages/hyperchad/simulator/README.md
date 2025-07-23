# MoosicBox HyperChad Simulator

Simulation framework for HyperChad applications using simvar and switchy for deterministic testing.

## Features

* **Deterministic Testing**: Reproducible UI behavior across all renderers
* **Multi-Renderer Support**: Test HTML, Vanilla JS, egui, and FLTK renderers
* **Performance Benchmarking**: Consistent measurements without external factors
* **Integration Validation**: Test hyperchad apps against simulated backends
* **CI/CD Integration**: Automated simulation runs in build pipelines

## Usage

```rust
use hyperchad_simulator::{HyperChadSimulator, RendererType};
use hyperchad_test_utils::TestPlan;

let simulator = HyperChadSimulator::new()
    .with_renderer(RendererType::VanillaJs)
    .with_mock_data(test_data);

let plan = TestPlan::new()
    .navigate_to("/login")
    .fill_form(login_form)
    .click("#submit")
    .wait_for(WaitCondition::url_contains("/dashboard"))
    .assert_dom("#welcome-message", DomMatcher::visible());

let result = simulator.run_test_plan(plan)?;
```