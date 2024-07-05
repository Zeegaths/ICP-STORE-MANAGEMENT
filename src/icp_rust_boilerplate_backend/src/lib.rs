#[macro_use]
extern crate serde; // Import the serde library for serialization and deserialization

use candid::{Decode, Encode}; // Import Decode and Encode from the candid library
use ic_cdk::api::time; // Import the time API from ic_cdk
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory}; // Import memory management structures from ic_stable_structures
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable}; // Import stable structures
use std::{borrow::Cow, cell::RefCell}; // Import Cow and RefCell from the standard library

type Memory = VirtualMemory<DefaultMemoryImpl>; // Type alias for VirtualMemory using DefaultMemoryImpl
type IdCell = Cell<u64, Memory>; // Type alias for Cell storing u64 with Memory

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)] // Derive macros for InventoryItem struct
struct InventoryItem {
    id: u64, // Unique identifier for the item
    name: String, // Name of the item
    quantity: u32, // Quantity of the item
    price: f64, // Price of the item
    created_at: u64, // Timestamp of when the item was created
    updated_at: Option<u64>, // Optional timestamp of when the item was last updated
}

// Implement the Storable trait for InventoryItem struct
impl Storable for InventoryItem {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap()) // Serialize the InventoryItem struct to bytes
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap() // Deserialize bytes to an InventoryItem struct
    }
}

// Implement the BoundedStorable trait for InventoryItem struct
impl BoundedStorable for InventoryItem {
    const MAX_SIZE: u32 = 1024; // Maximum size of the serialized InventoryItem in bytes
    const IS_FIXED_SIZE: bool = false; // Indicates that the size is not fixed
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter")
    );

    static INVENTORY: RefCell<StableBTreeMap<u64, InventoryItem, Memory>> =
        RefCell::new(StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));
}

#[derive(candid::CandidType, Serialize, Deserialize, Default)] // Derive macros for InventoryPayload struct
struct InventoryPayload {
    name: String, // Name of the item
    quantity: u32, // Quantity of the item
    price: f64, // Price of the item
}

#[ic_cdk::query] // Mark the function as a query method
fn get_item(id: u64) -> Result<InventoryItem, Error> {
    match _get_item(&id) {
        Some(item) => Ok(item), // Return the item if found
        None => Err(Error::NotFound {
            msg: format!("An item with id={} not found", id), // Return an error if the item is not found
        }),
    }
}

#[ic_cdk::query] // Mark the function as a query method
fn list_items() -> Vec<InventoryItem> {
    INVENTORY.with(|inventory| {
        inventory.borrow().iter().map(|(_, item)| item.clone()).collect() // Collect and return all items
    })
}

#[ic_cdk::update] // Mark the function as an update method
fn add_item(payload: InventoryPayload) -> Result<InventoryItem, Error> {
    // Validate input payload
    if payload.name.is_empty() {
        return Err(Error::InvalidInput { msg: "Name must be provided and non-empty".to_string() });
    }
    if payload.quantity == 0 {
        return Err(Error::InvalidInput { msg: "Quantity must be greater than zero".to_string() });
    }
    if payload.price <= 0.0 {
        return Err(Error::InvalidInput { msg: "Price must be greater than zero".to_string() });
    }

    // Increment the ID counter
    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("Cannot increment ID counter");

    // Create a new InventoryItem struct
    let item = InventoryItem {
        id,
        name: payload.name,
        quantity: payload.quantity,
        price: payload.price,
        created_at: time(), // Set the creation timestamp
        updated_at: None, // No update timestamp yet
    };

    // Insert the new item into inventory
    do_insert(&item);

    Ok(item)
}

#[ic_cdk::update] // Mark the function as an update method
fn update_item(id: u64, payload: InventoryPayload) -> Result<InventoryItem, Error> {
    // Validate input payload
    if payload.name.is_empty() {
        return Err(Error::InvalidInput { msg: "Name must be provided and non-empty".to_string() });
    }
    if payload.quantity == 0 {
        return Err(Error::InvalidInput { msg: "Quantity must be greater than zero".to_string() });
    }
    if payload.price <= 0.0 {
        return Err(Error::InvalidInput { msg: "Price must be greater than zero".to_string() });
    }

    // Fetch the existing item
    match INVENTORY.with(|inventory| inventory.borrow().get(&id)) {
        Some(mut item) => {
            // Update item details
            item.name = payload.name;
            item.quantity = payload.quantity;
            item.price = payload.price;
            item.updated_at = Some(time()); // Set the update timestamp

            // Update the item in inventory
            do_insert(&item);

            Ok(item)
        }
        None => Err(Error::NotFound {
            msg: format!("Couldn't update an item with id={}. Item not found.", id),
        }),
    }
}

// Helper method to perform insert operation
fn do_insert(item: &InventoryItem) {
    INVENTORY.with(|inventory| inventory.borrow_mut().insert(item.id, item.clone()));
}

#[ic_cdk::update] // Mark the function as an update method
fn delete_item(id: u64) -> Result<InventoryItem, Error> {
    // Remove the item from inventory
    match INVENTORY.with(|inventory| inventory.borrow_mut().remove(&id)) {
        Some(item) => Ok(item), // Return the deleted item if found
        None => Err(Error::NotFound {
            msg: format!("Couldn't delete an item with id={}. Item not found.", id),
        }),
    }
}

#[derive(candid::CandidType, Deserialize, Serialize)] // Derive macros for the Error enum
enum Error {
    NotFound { msg: String }, // Error variant for not found
    InvalidInput { msg: String }, // Error variant for invalid input
}

// Helper method to get an item by ID, used in get_item and update_item
fn _get_item(id: &u64) -> Option<InventoryItem> {
    INVENTORY.with(|inventory| inventory.borrow().get(id))
}

// Generate candid interface
ic_cdk::export_candid!();
