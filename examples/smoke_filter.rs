use newboot::{filter, Call, Settings};

#[derive(Settings)]
pub struct SmokeFilterSettings {
  #[setting(default = "true")]
  keep: bool,
}

#[filter]
pub fn smoke_filter(Call { settings, .. }: Call<SmokeFilterSettings>) -> newboot::Result<bool> {
  newboot::log::debug!("keep {}", settings.keep);
  newboot::metrics::increment("smoke_filter");
  Ok(settings.keep)
}

fn main() {
  let _ = smoke_filter_manifest("smoke-filter", "0.1.0", "wasm32 compile smoke");
  #[cfg(not(target_arch = "wasm32"))]
  let _ = newboot::test::run(newboot::test::Run {
    function: smoke_filter,
    message: newboot::Message::new("t", Vec::new()),
    settings: &[],
  });
}
