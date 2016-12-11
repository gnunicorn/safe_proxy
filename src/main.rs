extern crate iron;
extern crate url;
extern crate safe_core;
extern crate maidsafe_utilities;
extern crate hyper;
extern crate mime_guess;
extern crate thread_id;

#[macro_use]
extern crate lazy_static;

use iron::prelude::*;
use iron::status;
use url::Host;
use iron::Handler;

use iron::headers::{Headers, ContentType};
use iron::mime::{Mime, TopLevel, SubLevel};
use mime_guess::get_mime_type;


use safe_core::core::client::Client;
use safe_core::dns::dns_operations::DnsOperations;
use safe_core::nfs::helper::file_helper::FileHelper;
use safe_core::nfs::directory_listing::DirectoryListing;
use safe_core::nfs::helper::directory_helper::DirectoryHelper;
use safe_core::nfs::metadata::directory_key::DirectoryKey;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;



static CLIENTSIZE: usize = 10;

lazy_static! {
    static ref CLIENTS: Mutex<HashMap<usize, Arc<Mutex<Client>>>> = Mutex::new(HashMap::new());
}


pub fn get_final_subdirectory(client: Arc<Mutex<Client>>,
                              tokens: &Vec<&str>,
                              starting_directory: Option<&DirectoryKey>)
                              -> DirectoryListing {

    let dir_helper = DirectoryHelper::new(client);

    let mut current_dir_listing = match starting_directory {
        Some(directory_key) => {
            dir_helper.get(directory_key).unwrap()
        }
        None => {
            dir_helper.get_user_root_directory_listing().unwrap()
        }
    };

    for it in tokens.iter().map(|s| s.to_string()) {

        current_dir_listing = {
            let current_dir_metadata = current_dir_listing.get_sub_directories()
                .iter()
                .find(|a| a.get_name() == it).unwrap();
            dir_helper.get(current_dir_metadata.get_key()).unwrap()
        };
    }

    current_dir_listing
}




fn fetch_file(client:  Arc<Mutex<Client>>,
			  long_name: &str,
			  service_name: &str,
			  path: Vec<&str>,
			  file_name: &str) -> Vec<u8> {
	let dns_operations = DnsOperations::new_unregistered(client.clone());
    let directory_key = dns_operations.get_service_home_directory_key(long_name, service_name, None).unwrap();
    let file_dir = get_final_subdirectory(client.clone(), &path, Some(&directory_key));
    let file = file_dir.find_file(&file_name).unwrap();


	let mut file_helper = FileHelper::new(client);
	let mut reader = file_helper.read(&file).unwrap();
	let size = reader.size();
	reader.read(0, size).unwrap()
}

#[derive(Clone)]
struct ProxyHandler { }

impl ProxyHandler {
	fn new() -> Self{
		ProxyHandler { }
	}

	fn get_client(&self) -> Arc<Mutex<Client>> {
		let mut cache = CLIENTS.lock().unwrap();
		// let's keep around up to 8 clients and distribute them kinda randomly..
		let id = thread_id::get() % CLIENTSIZE;
		println!("we are fetching for {}", id);
		cache.entry(id)
			 .or_insert_with(|| Arc::new(Mutex::new(Client::create_unregistered_client().unwrap())));
		cache.get(&id).unwrap().clone()
	}
}

impl Handler for ProxyHandler {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
		let client = self.get_client();
    	let ref url = req.url;
    	if let Host::Domain(domain) = url.host() {

    		let mut domain_parts = domain.rsplit(".");
    		let long_name : &str = domain_parts.next().unwrap();
    		let service = {
    			let mut services = domain_parts.collect::<Vec<&str>>();
    			if services.len() == 0 {
    				"www".to_string()
    			} else {
    				services.reverse();
    				services.join(".")
    			}
    		};


    		let mut path = url.path().clone(); 
    		let file_name = {
    			let name = path.pop().unwrap_or("");
    			if name == "" {
    				"index.html"
    			} else {
    				name
    			}
    		};
    		let mtype = get_mime_type(file_name.clone().rsplit(".").next().unwrap_or("html"));

    		// safe://invoice-app.nobackend-example/nobackend-examples/safenet/index.html
    		// FIXME: add etag support!
    		let file = fetch_file(client, "nobackend-example", "invoice-app", path, file_name);
        	let mut resp = Response::with((status::Ok, file));
			resp.headers.set(ContentType(mtype));
			Ok(resp)
    	} else {
    		Ok(Response::with((status::NotFound, "Can't connect with IP")))
    	}
    }
}



fn main() {

	fn kickstart_clients() {
		let mut cache = CLIENTS.lock().unwrap();
	    for i in 0..CLIENTSIZE {
			cache.insert(i as usize, Arc::new(Mutex::new(Client::create_unregistered_client().unwrap())));
	    }
	}

    maidsafe_utilities::log::init(true).unwrap();

    let proxy = ProxyHandler::new();
    let _server = Iron::new(proxy).http("localhost:3000").unwrap();
    println!("On 3000");
    kickstart_clients();
    println!("clients kickstarted");
}