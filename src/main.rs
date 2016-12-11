#[macro_use]
extern crate iron;
extern crate url;
extern crate safe_core;
extern crate maidsafe_utilities;
extern crate hyper;
extern crate mime_guess;
extern crate thread_id;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate unwrap;

use iron::prelude::*;
use iron::status;
use url::Host;

use iron::headers::ContentType;
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

    let mut current_dir_listing = unwrap!(match starting_directory {
        Some(directory_key) => dir_helper.get(directory_key),
        None => dir_helper.get_user_root_directory_listing()
    });

    for it in tokens.iter().map(|s| s.to_string()) {

        current_dir_listing = {
            let current_dir_metadata = unwrap!(current_dir_listing.get_sub_directories()
                .iter()
                .find(|a| a.get_name() == it));
            unwrap!(dir_helper.get(current_dir_metadata.get_key()))
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
    let directory_key = unwrap!(dns_operations.get_service_home_directory_key(long_name, service_name, None));
    let file_dir = get_final_subdirectory(client.clone(), &path, Some(&directory_key));
    let file = unwrap!(file_dir.find_file(&file_name));


	let mut file_helper = FileHelper::new(client);
	let mut reader = unwrap!(file_helper.read(&file));
	let size = reader.size();
	unwrap!(reader.read(0, size))
}


fn get_client() -> Arc<Mutex<Client>> {
	let mut cache = unwrap!(CLIENTS.lock());
	// let's keep around up to 8 clients and distribute them kinda randomly..
	let id = thread_id::get() % CLIENTSIZE;
	println!("we are fetching for {}", id);
	cache.entry(id)
		 .or_insert_with(|| Arc::new(Mutex::new(Client::create_unregistered_client().unwrap())));
	unwrap!(cache.get(&id)).clone()
}


fn proxy_request(req: &mut Request) -> IronResult<Response> {
	let client = get_client();
	let ref url = req.url;
	if let Host::Domain(domain) = url.host() {

		let mut domain_parts = domain.rsplit(".");
		let tld = domain_parts.next();
		let (long_name, service) : (&str, String) = if tld.is_some()  {
    		(iexpect!(domain_parts.next(), status::BadRequest), // long name
			{
    			let mut services = domain_parts.collect::<Vec<&str>>();
    			if services.len() > 0 {
    				// put together service name
    				services.reverse();
    				services.join(".")
    			} else {
    				// or default to 'www'
    				"www".to_string()
    			}
    		})
		} else {
			("nobackend-example", "invoice-app".to_string()) 
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
		let file = fetch_file(client, &long_name, &service, path, file_name);
    	let mut resp = Response::with((status::Ok, file));
		resp.headers.set(ContentType(mtype));
		Ok(resp)
	} else {
		Ok(Response::with((status::NotFound, "Can't connect with IP")))
	}
}



fn main() {

	fn kickstart_clients() {
		let mut cache = unwrap!(CLIENTS.lock());
	    for i in 0..CLIENTSIZE {
			cache.insert(i as usize, Arc::new(Mutex::new(Client::create_unregistered_client().unwrap())));
	    }
	}

    unwrap!(maidsafe_utilities::log::init(true));

    let _server = unwrap!(Iron::new(proxy_request).http("localhost:3000"));
    println!("On 3000");
    kickstart_clients();
    println!("clients kickstarted");
}