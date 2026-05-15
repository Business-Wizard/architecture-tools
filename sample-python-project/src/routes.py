from decimal import Decimal

from src.customer import Customer
from src.domain_order import Order


def post_order(
    order_id: str, customer_id: str, name: str, email: str, amount: str
) -> dict[str, str]:
    customer = Customer(customer_id, name, email)
    order = Order(order_id, customer, Decimal(amount))  # violation: constructs Order directly
    order.confirm()
    return {"order_id": order.order_id, "status": order.status}
