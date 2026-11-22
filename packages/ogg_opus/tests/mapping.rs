use moosicbox_ogg_opus::OpusHead;

#[test]
fn canonical_family1_all_channel_counts() {
    let layouts: &[(u8, u8, &[u8])] = &[
        (1, 0, &[0]),
        (1, 1, &[0, 1]),
        (2, 1, &[0, 2, 1]),
        (2, 2, &[0, 1, 2, 3]),
        (3, 2, &[0, 4, 1, 2, 3]),
        (4, 2, &[0, 4, 1, 2, 3, 5]),
        (4, 3, &[0, 4, 1, 2, 3, 5, 6]),
        (5, 3, &[0, 6, 1, 2, 3, 4, 5, 7]),
    ];
    for (channels, &(streams, coupled, mapping)) in (1_u8..=8).zip(layouts) {
        let head = OpusHead::new_family1(channels, 312, 48000).unwrap();
        assert_eq!(head.mapping_family, 1);
        assert_eq!(head.stream_count, streams);
        assert_eq!(head.coupled_count, coupled);
        assert_eq!(head.channel_mapping, mapping);
        assert_eq!(head.pre_skip, 312);
        assert_eq!(OpusHead::parse(&head.encode().unwrap()).unwrap(), head);
    }
    for channels in [0, 9, 255] {
        assert!(OpusHead::new_family1(channels, 0, 48000).is_err());
    }
}

#[test]
fn family255_explicit_map_validated_by_encode() {
    let mut head = OpusHead {
        channels: 4,
        pre_skip: 312,
        input_sample_rate: 48000,
        output_gain: 0,
        mapping_family: 255,
        stream_count: 2,
        coupled_count: 1,
        channel_mapping: vec![0, 1, 2, 255],
    };
    assert_eq!(OpusHead::parse(&head.encode().unwrap()).unwrap(), head);
    head.channel_mapping[3] = 3;
    assert!(head.encode().is_err());
    head.channel_mapping[3] = 0; // Duplicated output is valid.
    assert!(head.encode().is_ok());
    head.channel_mapping.pop();
    assert!(head.encode().is_err());
}
