use newboot::{map, Call, Message, Settings};

#[derive(Settings)]
pub struct SmokeMapSettings {
  #[setting(default = "1")]
  scale: u32,
}

#[map]
pub fn smoke_map(Call { message, settings }: Call<SmokeMapSettings>) -> newboot::Result<Message> {
  newboot::log::debug!("scale {}", settings.scale);
  newboot::metrics::increment("smoke_map");
  Ok(message)
}

fn main() {
  let _ = smoke_map_manifest("smoke-map", "0.1.0", "wasm32 compile smoke");
  #[cfg(not(target_arch = "wasm32"))]
  let _ = newboot::test::run(newboot::test::Run {
    function: smoke_map,
    message: Message::new("t", Vec::new()),
    settings: &[],
  });
}
