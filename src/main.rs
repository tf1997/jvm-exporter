mod routes;
mod monitor;

mod deploy{
    pub mod deploy;
}
mod metrics {
    pub mod metrics;
    pub mod collect;
    pub mod timer;
}

fn main() {
    monitor::main()
}
