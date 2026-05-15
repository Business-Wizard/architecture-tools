"""Order domain model and use cases."""

from decimal import Decimal

from .customer import Customer


class Order:
    """Represents an order."""

    def __init__(self, order_id: str, customer: Customer, total: Decimal) -> None:
        self.order_id = order_id
        self.customer = customer
        self.total = total
        self.status = "pending"

    def confirm(self) -> None:
        """Confirm the order."""
        self.status = "confirmed"
        send_confirmation_email(self.customer.email, self.order_id)

    def cancel(self) -> None:
        """Cancel the order."""
        self.status = "cancelled"
        send_cancellation_email(self.customer.email, self.order_id)

    def get_customer_name(self) -> str:
        """Get the customer name for this order."""
        return self.customer.get_full_name()


def send_confirmation_email(email: str, order_id: str) -> None:
    """Send confirmation email to customer."""
    print(f"Sending confirmation for {order_id} to {email}")


def send_cancellation_email(email: str, order_id: str) -> None:
    """Send cancellation email to customer."""
    print(f"Sending cancellation for {order_id} to {email}")


def process_order(order: Order) -> None:
    """Process an order through the system."""
    order.confirm()
    log_order_processed(order.order_id)


def log_order_processed(order_id: str) -> None:
    """Log that an order was processed."""
    print(f"Order {order_id} processed")
