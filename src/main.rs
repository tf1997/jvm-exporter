mod jvm_export;
mod routes;
mod monitor;

mod deploy{
    pub mod deploy;
}
mod metrics {
    pub mod metrics;
    pub mod collect;
}

fn main() {
    monitor::main()
}
