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
    //max_retries: u32,
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
            //max_retries: 0,
            health_check_interval: Duration::from_secs(0),
            dead_servers: Vec::new(),
        }
    }
    fn update(&mut self, path: &str, port: &str) -> io::Result<&Config> {
        let path = Path::new(path);
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut servers: Vec<hyper::Uri> = Vec::new();
        let mut weights: Vec<u32> = Vec::new();
        let mut max_connections: Vec<u32> = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.starts_with("load balancer:") {
                let load_balancer =
                    (String::from("http://127.0.0.1:") + &String::from(port)).parse::<hyper::Uri>(); //CLI input

                let load_balancer = match load_balancer {
                    Ok(load_balancer) => load_balancer,
                    _ => {
                        //If no CLI input, take addr from config.yaml
                        let lobal = line
                            .trim_start_matches("load balancer:")
                            .trim()
                            .parse::<hyper::Uri>();

                        let lobal = match lobal {
                            Ok(lobal) => lobal,
                            Err(_) => "http://127.0.0.1:8000".parse::<hyper::Uri>().unwrap(), //Default address for load balancer
                        };
                        lobal
                    }
                };

                self.load_balancer = load_balancer;
            } else if line.starts_with("algorithm:") {
                let algo = line.trim_start_matches("algorithm:").trim();
                self.algo = get_algo(algo);
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
            // } else if line.starts_with("max retries:") {
            //     let max_retries = line.trim_start_matches("max retries:").trim();
            //     self.max_retries = max_retries.parse::<u32>().expect("Invalid timeout");
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
            Command::new("start").about("Start the load balancer").arg(
                Arg::new("port")
                    .short('p')
                    .long("port")
                    .default_value("8000")
                    .help("Starts load balancer at specified port"),
            ), // .arg(
               //     Arg::new("algorithm")
               //         .short('a')
               //         .long("algorithm")
               //         .default_value("round_robin")
               //         .help(
               //             "Starts load balancer with specified algorithm\n
               //     Available algorithms: round_robin, weighted_round_robin",
               //         ),
               // ),
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
        config.update("config.yaml", "8000")?;
        println!("{} servers listed", config.servers.len());

        return Ok(());
    }

    match res.subcommand_name() {
        Some("start") => {
            println!("Starting load balancer");
            let start_args = res.subcommand_matches("start").unwrap();
            let path = res.get_one::<String>("path").unwrap();
            let port = start_args.get_one::<String>("port").unwrap();

            config.update(path, port)?;
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
