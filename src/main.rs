mod monitor;
mod routes;
mod config;

mod metrics {
    pub mod collect;
    pub mod metrics;
    pub mod timer;
}

fn main() {
    monitor::main()
}
