```rust
fn main() {
    println!("hello");
}
```

```text
fenced with tildes
```

- [x] **Implement stereo weight decoding:**

    ```rust
    let w1_q13 = STEREO_WEIGHT_TABLE_Q13[wi1]
        + (((i32::from(STEREO_WEIGHT_TABLE_Q13[wi1 + 1])
            - i32::from(STEREO_WEIGHT_TABLE_Q13[wi1]))
            * 6554)
            >> 16)
            * i32::from(2 * i3 + 1);

    let w0_q13 = STEREO_WEIGHT_TABLE_Q13[wi0]
        + (((i32::from(STEREO_WEIGHT_TABLE_Q13[wi0 + 1])
            - i32::from(STEREO_WEIGHT_TABLE_Q13[wi0]))
            * 6554)
            >> 16)
            * i32::from(2 * i1 + 1)
        - w1_q13;
    ```
