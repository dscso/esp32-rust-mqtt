#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod sta;

use crate::sta::connection;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::dns::DnsQueryType;
use embassy_net::{Config, ConfigV6, Ipv6Address, Ipv6Cidr, Stack, StackResources, StaticConfigV6};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal as hal;
use esp_println::println;
use esp_wifi::wifi::{WifiDevice, WifiStaDevice};
use esp_wifi::{initialize, EspWifiInitFor};
use hal::clock::ClockControl;
use hal::rng::Rng;
use hal::{embassy, peripherals::Peripherals, prelude::*, timer::TimerGroup};
use log::info;
use mqtt_server::distributor::{InnerDistributor, InnerDistributorMutex};
use static_cell::make_static;

use mqtt_server::socket::listen;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");
const MAX_CONNECTIONS: usize = 16;

#[main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger(log::LevelFilter::Info);

    let peripherals = Peripherals::take();

    let system = peripherals.SYSTEM.split();
    let clocks = ClockControl::max(system.clock_control).freeze();
    let mut rng = Rng::new(peripherals.RNG);

    let (seed_hi, seed_lo) = (rng.random(), rng.random());
    let seed = (seed_hi as u64) << 32 | seed_lo as u64;
    info!("Seed: {:x}", seed);

    #[cfg(target_arch = "xtensa")]
    let timer = hal::timer::TimerGroup::new(peripherals.TIMG1, &clocks, None).timer0;
    #[cfg(target_arch = "riscv32")]
    let timer = hal::systimer::SystemTimer::new(peripherals.SYSTIMER).alarm0;
    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        rng,
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    let wifi = peripherals.WIFI;
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&init, wifi, WifiStaDevice).unwrap();

    let timer_group0 = TimerGroup::new_async(peripherals.TIMG0, &clocks);
    embassy::init(&clocks, timer_group0);

    let mut config = Config::dhcpv4(Default::default());
    // might be removed if smoltcp gets slaac support
    const IPV6_ADDR: Option<&str> = option_env!("IPV6");
    if let Some(ipv6) = IPV6_ADDR {
        let ip = Ipv6Address::from_str(ipv6)
            .expect("ipv6 addr configured in IPV6 environment variable is invalid");

        config.ipv6 = ConfigV6::Static(StaticConfigV6 {
            address: Ipv6Cidr::new(ip, 64),
            gateway: None,
            dns_servers: Default::default(),
        });
    }
    // Init network stack
    let stack = &*make_static!(Stack::new(
        wifi_interface,
        config,
        make_static!(StackResources::<{ 5 + MAX_CONNECTIONS }>::new()),
        seed
    ));

    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(stack)).ok();

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    let distributor = &*make_static!(InnerDistributorMutex::new(InnerDistributor::default()));
    // spawn listeners for concurrent connections
    for i in 0..MAX_CONNECTIONS {
        spawner
            .spawn(listen_task(stack, i, 1883, distributor))
            .ok();
    }

    println!("Waiting to get IPv4 address...");
    loop {
        if let Some(config) = stack.config_v4() {
            println!("Got IPv4: {}", config.address);
            if let Some(config) = stack.config_v6() {
                println!("Got IPv6: {}", config.address)
            }
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    loop {
        if let Some(config) = stack.config_v6() {
            println!("Got IPv6: {}", config.address)
        }
        let res = stack.dns_query("google.de", DnsQueryType::A).await;
        info!("DNS query result: {:?}", res);
        Timer::after(Duration::from_secs(30)).await;
    }
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>) {
    stack.run().await
}

#[embassy_executor::task(pool_size = MAX_CONNECTIONS)]
async fn listen_task(
    stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>,
    id: usize,
    port: u16,
    distributor: &'static InnerDistributorMutex<MAX_CONNECTIONS>,
) {
    listen(stack, id, port, distributor).await
}
