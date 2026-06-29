pub trait Migration {
    fn version(&self) -> u64;
    fn description(&self) -> &'static str;
    fn run(&self) -> Result<(), String>;
}

pub struct From30To31;
impl Migration for From30To31 {
    fn version(&self) -> u64 { 1 }
    fn description(&self) -> &'static str { "3.0 → 3.1: Update poster link format" }
    fn run(&self) -> Result<(), String> {
        tracing::info!("Running migration 3.0 → 3.1");
        Ok(())
    }
}

pub struct From31To32;
impl Migration for From31To32 {
    fn version(&self) -> u64 { 2 }
    fn description(&self) -> &'static str { "3.1 → 3.2: Database schema migration" }
    fn run(&self) -> Result<(), String> {
        tracing::info!("Running migration 3.1 → 3.2");
        Ok(())
    }
}

pub fn all_migrations() -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(From30To31),
        Box::new(From31To32),
    ]
}
