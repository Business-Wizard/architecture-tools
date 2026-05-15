from decimal import Decimal

from src.customer import Customer
from src.domain_order import Order


def create_order(
    order_id: str, customer_id: str, name: str, email: str, total: Decimal
) -> Order:
    customer = Customer(customer_id, name, email)
    return Order(order_id, customer, total)  # violation: constructs Order directly


def confirm_order(order: Order) -> None:
    order.confirm()
    charge_id = order.charge()
    print(f"Confirmed and charged: {charge_id}")
