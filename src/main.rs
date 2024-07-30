use clap::{command, Arg, ArgAction, Command};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

mod lb;

#[derive(Debug)]
struct Config {
    load_balancer: hyper::Uri,
    algo: Algorithm,
    servers: Vec<Server>,
    timeout: Duration,
    health_check_interval: Duration,
    dead_servers: Vec<Server>,
}

#[derive(Debug, Clone, PartialEq)]
struct Server {
    addr: hyper::Uri,
    weight: u32,
    response_time: Duration,
    connections: u32,
    max_connections: u32,
}

impl Server {
    fn new(addr: hyper::Uri, weight: u32, max_connections: u32) -> Self {
        Server {
            addr,
            weight,
            max_connections,
            response_time: Duration::from_secs(0),
            connections: 0,
        }
    }
}

impl Config {
    fn new() -> Self {
        Config {
            load_balancer: "http://127.0.0.1:8000".parse::<hyper::Uri>().unwrap(), // default address for load balancer
            algo: Algorithm::RoundRobin, // using round robin as default algorithm
            servers: Vec::new(),
            timeout: Duration::from_secs(0),
            health_check_interval: Duration::from_secs(0),
            dead_servers: Vec::new(),
        }
    }
    fn update(&mut self, path: &str, addr: Option<&str>, algorithm: Option<&str>) -> io::Result<&Config> {
        let path = Path::new(path);
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut servers: Vec<hyper::Uri> = Vec::new();
        let mut weights: Vec<u32> = Vec::new();
        let mut max_connections: Vec<u32> = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.starts_with("load balancer:") {
                let addr = match addr {
                    Some(addr) => addr, //CLI input
                    None => line //If no CLI input take from config.yaml
                        .trim_start_matches("load balancer:")
                        .trim()
                };

                let load_balancer = String::from(addr).parse::<hyper::Uri>();

                let load_balancer = match load_balancer {
                    Ok(load_balancer) => load_balancer,
                    Err(_) => "http://127.0.0.1:8000".parse::<hyper::Uri>().unwrap(), //Default address for load balancer
                };
                self.load_balancer = load_balancer;
            } else if line.starts_with("algorithm:") {
                let algorithm = match algorithm {
                    Some(algorithm) => algorithm, //CLI input
                    None => line //If no CLI input take from config.yaml
                        .trim_start_matches("algorithm:")
                        .trim(),
                };

                self.algo = get_algo(algorithm);
            } else if line.starts_with("servers:") {
                let servers_str = line.trim_start_matches("servers:").trim();
                servers = servers_str
                    .split(",")
                    .map(|server| server.trim().parse::<hyper::Uri>().expect("Invalid URI"))
                    .collect();
            } else if line.starts_with("weights:") {
                let weights_str = line.trim_start_matches("weights:").trim();
                weights = weights_str
                    .split(",")
                    .map(|weight| weight.trim().parse::<u32>().expect("Invalid weight"))
                    .collect();
                // println!("{:?}", weights);
            } else if line.starts_with("max connections:") {
                let max_connections_str = line.trim_start_matches("max connections:").trim();
                max_connections = max_connections_str
                    .split(",")
                    .map(|max_connection| {
                        max_connection
                            .trim()
                            .parse::<u32>()
                            .expect("Invalid max connection")
                    })
                    .collect();
            } else if line.starts_with("timeout:") {
                let timeout = line.trim_start_matches("timeout:").trim();
                self.timeout =
                    Duration::from_secs(timeout.parse::<u64>().expect("Invalid timeout"));
            } else if line.starts_with("health check interval:") {
                let health_check_interval =
                    line.trim_start_matches("health check interval:").trim();
                self.health_check_interval = Duration::from_secs(
                    health_check_interval
                        .parse::<u64>()
                        .expect("Invalid helth check interval"),
                );
            }
        }

        for i in 0..servers.len() {
            self.servers.push(Server::new(
                servers[i].clone(),
                weights[i],
                max_connections[i],
            ));
        }

        Ok(self)
    }
}

#[derive(Debug, Clone)]
enum Algorithm {
    RoundRobin,
    WeightedRoundRobin,
    LeastConnections,
    WeightedLeastConnections,
    LeastResponseTime,
    WeightedLeastResponseTime,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut config = Config::new();
    config.update("config.yaml", None, None)?;

    let res = command!()
        .about(
            r#"
 ________  ___  ________  ________  ___  ___  ________      
|\   ____\|\  \|\   __  \|\   ____\|\  \|\  \|\   ____\     
\ \  \___|\ \  \ \  \|\  \ \  \___|\ \  \\\  \ \  \___|_    
 \ \  \    \ \  \ \   _  _\ \  \    \ \  \\\  \ \_____  \   
  \ \  \____\ \  \ \  \\  \\ \  \____\ \  \\\  \|____|\  \  
   \ \_______\ \__\ \__\\ _\\ \_______\ \_______\____\_\  \ 
    \|_______|\|__|\|__|\|__|\|_______|\|_______|\_________\
    "#,
        )
        .subcommand(
            Command::new("start")
                .about("Start the load balancer")
                .arg(Arg::new("address")
                    .short('u')
                    .long("address")
                    .help("Starts load balancer at specified address")
        )
                .arg(Arg::new("algorithm").short('a').long("algorithm").help(
                    "Starts load balancer with specified algorithm
Available algorithms: round_robin, weighted_round_robin
Default value: round_robin",
                )),
        )
        .subcommand(Command::new("stop").about("Stop the load balancer"))
        .arg(
            Arg::new("path")
                .long("path")
                .default_value("config.yaml")
                .help("Specify path to config file"),
        )
        .arg(
            Arg::new("server-count")
                .short('s')
                .long("server-count")
                .help("Shows number of listed servers")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    if *res.get_one::<bool>("server-count").unwrap() {
        println!("{} servers listed", config.servers.len());

        return Ok(());
    }
    let lb_string = &config.load_balancer.to_string();
    match res.subcommand_name() {
        Some("start") => {
            println!("Starting load balancer");
            let start_args = res.subcommand_matches("start").unwrap();
            let path = res.get_one::<String>("path").unwrap();
            let address = match start_args.get_one::<&str>("address"){
                Some(addr) => Some(*addr),
                None => Some(lb_string.as_str())
            };
            let algo = match start_args.get_one::<&str>("algorithm"){
                Some(algo) => Some(*algo),
                None => Some(get_algo_rev(config.algo.clone())),
            };

            config.update(path, address, algo)?; //Update config with user input
            drop(lb::start_lb(config));
        }
        // Some("stop") => {
        //     println!("Stopping load balancer");
        //     drop(lb::stop_lb(config));
        // },
        _ => println!("Invalid command"),
    }

    Ok(())
}

fn get_algo(algo: &str) -> Algorithm {
    match algo {
        "round_robin" => Algorithm::RoundRobin,
        "weighted_round_robin" => Algorithm::WeightedRoundRobin,
        "least_connections" => Algorithm::LeastConnections,
        "weighted_least_connections" => Algorithm::WeightedLeastConnections,
        "least_response_time" => Algorithm::LeastResponseTime,
        "weighted_least_response_time" => Algorithm::WeightedLeastResponseTime,
        _ => Algorithm::RoundRobin, // Default algorithms
    }
}

fn get_algo_rev<'a>(algo: Algorithm) -> &'a str {
    match algo {
        Algorithm::RoundRobin => "round_robin",
        Algorithm::WeightedRoundRobin => "weighted_round_robin",
        Algorithm::LeastConnections => "least_connections",
        Algorithm::WeightedLeastConnections => "weighted_least_connections",
        Algorithm::LeastResponseTime => "least_response_time",
        Algorithm::WeightedLeastResponseTime => "weighted_least_response_time",
    }
}