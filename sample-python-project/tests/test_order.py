"""Tests for order module."""

from decimal import Decimal

from src.customer import Customer
from src.domain_order import Order as DomainOrder
from src.order import Order


def test_order_creation() -> None:
    """Test that orders can be created."""
    customer = Customer("c1", "John Doe", "john@example.com")
    order = Order("o1", customer, Decimal("99.99"))
    assert order.order_id == "o1"
    assert order.status == "pending"


def test_order_confirmation() -> None:
    """Test that orders can be confirmed."""
    customer = Customer("c1", "John Doe", "john@example.com")
    order = Order("o1", customer, Decimal("99.99"))
    order.confirm()
    assert order.status == "confirmed"


def test_order_cancellation() -> None:
    """Test that orders can be cancelled."""
    customer = Customer("c1", "John Doe", "john@example.com")
    order = Order("o1", customer, Decimal("99.99"))
    order.cancel()
    assert order.status == "cancelled"


def test_domain_order_creation_should_start_pending() -> None:
    customer = Customer("c2", "Jane Smith", "jane@example.com")
    order = DomainOrder("o2", customer, Decimal("49.99"))  # violation: constructs Order directly
    assert order.status == "pending"
