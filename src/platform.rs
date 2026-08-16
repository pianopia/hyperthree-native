use anyhow::Result;

/// Print the host platform and every wgpu adapter visible to the process.
/// This is intentionally usable without opening a window so CI and support
/// reports can diagnose driver availability before launching a game.
pub fn print_diagnostics() -> Result<()> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapters = instance.enumerate_adapters(wgpu::Backends::all());

    println!(
        "os={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("wgpu_adapters={}", adapters.len());
    for (index, adapter) in adapters.iter().enumerate() {
        let info = adapter.get_info();
        println!(
            "adapter[{index}] backend={:?} type={:?} name={:?} driver={:?}",
            info.backend, info.device_type, info.name, info.driver
        );
    }
    if adapters.is_empty() {
        println!("warning=no GPU adapter found; native rendering cannot start");
    }
    Ok(())
}
