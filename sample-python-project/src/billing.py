"""Billing module for processing payments."""

from decimal import Decimal

from .order import Order


class BillingService:
    """Handles billing and payment processing."""

    def __init__(self, gateway_api_key: str) -> None:
        self.gateway_api_key = gateway_api_key

    def charge_order(self, order: Order, amount: Decimal) -> bool:
        """Charge the customer for an order."""
        if amount <= 0:
            return False
        customer_name = order.get_customer_name()
        customer_email = order.customer.email
        return process_payment(
            customer_name, customer_email, amount, self.gateway_api_key
        )

    def refund_order(self, order: Order) -> bool:
        """Refund an order."""
        return issue_refund(order.order_id, order.total, self.gateway_api_key)


def process_payment(
    customer_name: str, email: str, amount: Decimal, api_key: str
) -> bool:
    """Process a payment with the gateway."""
    print(f"Processing payment for {customer_name} ({email}): ${amount}")
    return True


def issue_refund(order_id: str, amount: Decimal, api_key: str) -> bool:
    """Issue a refund for an order."""
    print(f"Refunding order {order_id} for ${amount}")
    return True
