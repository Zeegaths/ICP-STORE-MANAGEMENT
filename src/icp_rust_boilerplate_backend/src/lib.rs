#[macro_use]
extern crate serde;
use candid::{Decode, Encode};
use ic_cdk::api::time;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct InventoryItem {
    id: u64,
    name: String,
    quantity: u32,
    price: f64,
    created_at: u64,
    updated_at: Option<u64>,
}

// Implement Storable trait for InventoryItem
impl Storable for InventoryItem {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

// Implement BoundedStorable trait for InventoryItem
impl BoundedStorable for InventoryItem {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
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

#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct InventoryPayload {
    name: String,
    quantity: u32,
    price: f64,
}

#[ic_cdk::query]
fn get_item(id: u64) -> Result<InventoryItem, Error> {
    match _get_item(&id) {
        Some(item) => Ok(item),
        None => Err(Error::NotFound {
            msg: format!("An item with id={} not found", id),
        }),
    }
}

#[ic_cdk::query]
fn list_items() -> Vec<InventoryItem> {
    INVENTORY.with(|inventory| {
        inventory.borrow().iter().map(|(_, item)| item.clone()).collect()
    })
}

#[ic_cdk::update]
fn add_item(payload: InventoryPayload) -> Option<InventoryItem> {
    let id = ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter");
    let item = InventoryItem {
        id,
        name: payload.name,
        quantity: payload.quantity,
        price: payload.price,
        created_at: time(),
        updated_at: None,
    };
    do_insert(&item);
    Some(item)
}

#[ic_cdk::update]
fn update_item(id: u64, payload: InventoryPayload) -> Result<InventoryItem, Error> {
    match INVENTORY.with(|inventory| inventory.borrow().get(&id)) {
        Some(mut item) => {
            item.name = payload.name;
            item.quantity = payload.quantity;
            item.price = payload.price;
            item.updated_at = Some(time());
            do_insert(&item);
            Ok(item)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't update an item with id={}. item not found",
                id
            ),
        }),
    }
}

// helper method to perform insert.
fn do_insert(item: &InventoryItem) {
    INVENTORY.with(|inventory| inventory.borrow_mut().insert(item.id, item.clone()));
}

#[ic_cdk::update]
fn delete_item(id: u64) -> Result<InventoryItem, Error> {
    match INVENTORY.with(|inventory| inventory.borrow_mut().remove(&id)) {
        Some(item) => Ok(item),
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't delete an item with id={}. item not found.",
                id
            ),
        }),
    }
}

#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
}

// a helper method to get an item by id. used in get_item/update_item
fn _get_item(id: &u64) -> Option<InventoryItem> {
    INVENTORY.with(|inventory| inventory.borrow().get(id))
}

// need this to generate candid
ic_cdk::export_candid!();
