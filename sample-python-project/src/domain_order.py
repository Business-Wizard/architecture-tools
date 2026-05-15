from decimal import Decimal

from src.customer import Customer
from src.stripe_client import StripeClient  # violation: infra in domain


class Order:
    def __init__(self, order_id: str, customer: Customer, total: Decimal) -> None:
        self.order_id = order_id
        self.customer = customer
        self.total = total
        self.status = "pending"
        self._stripe = StripeClient("demo_key")

    def confirm(self) -> None:
        self.status = "confirmed"

    def charge(self) -> str:
        return self._stripe.charge(int(self.total * 100), self.customer.email)

    def get_customer_name(self) -> str:
        return self.customer.get_full_name()
