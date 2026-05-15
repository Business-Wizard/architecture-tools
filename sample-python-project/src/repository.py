from decimal import Decimal

from src.customer import Customer
from src.domain_order import Order
from src.order_service import confirm_order  # violation: persistence imports service layer

_store: dict[str, Order] = {}


def save_order(
    order_id: str, customer_id: str, name: str, email: str, total: Decimal
) -> Order:
    customer = Customer(customer_id, name, email)
    order = Order(order_id, customer, total)  # violation: constructs Order directly
    _store[order_id] = order
    confirm_order(order)  # violation: calls upward into service layer
    return order


def find_order(order_id: str) -> Order | None:
    return _store.get(order_id)
