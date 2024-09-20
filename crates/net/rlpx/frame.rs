use aes::{
    cipher::{BlockEncrypt as _, KeyInit as _, StreamCipher as _},
    Aes256Enc,
};
use ethereum_rust_core::{rlp::encode::RLPEncode as _, H128};
use sha3::Digest as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::connection::RLPxState;

pub(crate) async fn write<S: AsyncWrite + std::marker::Unpin>(
    mut frame_data: Vec<u8>,
    state: &mut RLPxState,
    stream: &mut S,
) {
    let mac_aes_cipher = Aes256Enc::new_from_slice(&state.mac_key.0).unwrap();

    // header = frame-size || header-data || header-padding
    let mut header = Vec::with_capacity(32);
    let frame_size = frame_data.len().to_be_bytes();
    header.extend_from_slice(&frame_size[5..8]);

    // header-data = [capability-id, context-id]  (both always zero)
    let header_data = (0_u8, 0_u8);
    header_data.encode(&mut header);

    header.resize(16, 0);
    state.egress_aes.apply_keystream(&mut header[..16]);

    let header_mac_seed = {
        let mac_digest: [u8; 16] = state.egress_mac.clone().finalize()[..16]
            .try_into()
            .unwrap();
        let mut seed = mac_digest.into();
        mac_aes_cipher.encrypt_block(&mut seed);
        H128(seed.into()) ^ H128(header[..16].try_into().unwrap())
    };
    state.egress_mac.update(header_mac_seed);
    let header_mac = state.egress_mac.clone().finalize();
    header.extend_from_slice(&header_mac[..16]);

    // Write header
    stream.write_all(&header).await.unwrap();

    // Pad to next multiple of 16
    frame_data.resize(frame_data.len().next_multiple_of(16), 0);
    state.egress_aes.apply_keystream(&mut frame_data);
    let frame_ciphertext = frame_data;

    // Send frame
    stream.write_all(&frame_ciphertext).await.unwrap();

    // Compute frame-mac
    state.egress_mac.update(&frame_ciphertext);

    // frame-mac-seed = aes(mac-secret, keccak256.digest(egress-mac)[:16]) ^ keccak256.digest(egress-mac)[:16]
    let frame_mac_seed = {
        let mac_digest: [u8; 16] = state.egress_mac.clone().finalize()[..16]
            .try_into()
            .unwrap();
        let mut seed = mac_digest.into();
        mac_aes_cipher.encrypt_block(&mut seed);
        (H128(seed.into()) ^ H128(mac_digest)).0
    };
    state.egress_mac.update(frame_mac_seed);
    let frame_mac = state.egress_mac.clone().finalize();

    // Send frame-mac
    stream.write_all(&frame_mac[..16]).await.unwrap();
}

pub(crate) async fn read<S: AsyncRead + std::marker::Unpin>(
    state: &mut RLPxState,
    stream: &mut S,
) -> Vec<u8> {
    let mac_aes_cipher = Aes256Enc::new_from_slice(&state.mac_key.0).unwrap();

    // Receive the message's frame header
    let mut frame_header = [0; 32];
    stream.read_exact(&mut frame_header).await.unwrap();
    // Both are padded to the block's size (16 bytes)
    let (header_ciphertext, header_mac) = frame_header.split_at_mut(16);

    // Validate MAC header
    // header-mac-seed = aes(mac-secret, keccak256.digest(egress-mac)[:16]) ^ header-ciphertext
    let header_mac_seed = {
        let mac_digest: [u8; 16] = state.ingress_mac.clone().finalize()[..16]
            .try_into()
            .unwrap();
        let mut seed = mac_digest.into();
        mac_aes_cipher.encrypt_block(&mut seed);
        (H128(seed.into()) ^ H128(header_ciphertext.try_into().unwrap())).0
    };

    // ingress-mac = keccak256.update(ingress-mac, header-mac-seed)
    state.ingress_mac.update(header_mac_seed);

    // header-mac = keccak256.digest(egress-mac)[:16]
    let expected_header_mac = H128(
        state.ingress_mac.clone().finalize()[..16]
            .try_into()
            .unwrap(),
    );

    assert_eq!(header_mac, expected_header_mac.0);

    let header_text = header_ciphertext;
    state.ingress_aes.apply_keystream(header_text);

    // header-data = [capability-id, context-id]
    // Both are unused, and always zero
    assert_eq!(&header_text[3..6], &(0_u8, 0_u8).encode_to_vec());

    let frame_size: usize = u32::from_be_bytes([0, header_text[0], header_text[1], header_text[2]])
        .try_into()
        .unwrap();
    // Receive the hello message
    let padded_size = frame_size.next_multiple_of(16);
    let mut frame_data = vec![0; padded_size + 16];
    stream.read_exact(&mut frame_data).await.unwrap();
    let (frame_ciphertext, frame_mac) = frame_data.split_at_mut(padded_size);

    // check MAC
    #[allow(clippy::needless_borrows_for_generic_args)]
    state.ingress_mac.update(&frame_ciphertext);
    let frame_mac_seed = {
        let mac_digest: [u8; 16] = state.ingress_mac.clone().finalize()[..16]
            .try_into()
            .unwrap();
        let mut seed = mac_digest.into();
        mac_aes_cipher.encrypt_block(&mut seed);
        (H128(seed.into()) ^ H128(mac_digest)).0
    };
    state.ingress_mac.update(frame_mac_seed);
    let expected_frame_mac: [u8; 16] = state.ingress_mac.clone().finalize()[..16]
        .try_into()
        .unwrap();

    assert_eq!(frame_mac, expected_frame_mac);

    // decrypt frame
    state.ingress_aes.apply_keystream(frame_ciphertext);

    let (frame_data, _padding) = frame_ciphertext.split_at(frame_size);

    frame_data.to_vec()
}