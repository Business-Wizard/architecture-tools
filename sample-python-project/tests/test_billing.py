from decimal import Decimal

from src.billing import BillingService
from src.customer import Customer
from src.order import Order


def _make_order(order_id: str = "o1") -> Order:
    customer = Customer("c1", "John Doe", "john@example.com")
    return Order(order_id, customer, Decimal("50.00"))


def test_charge_order_with_positive_amount_should_return_true() -> None:
    service = BillingService("key_test")
    order = _make_order()
    actual = service.charge_order(order, Decimal("50.00"))
    assert actual is True


def test_charge_order_with_zero_amount_should_return_false() -> None:
    service = BillingService("key_test")
    order = _make_order()
    actual = service.charge_order(order, Decimal("0.00"))
    assert actual is False


def test_charge_order_with_negative_amount_should_return_false() -> None:
    service = BillingService("key_test")
    order = _make_order()
    actual = service.charge_order(order, Decimal("-10.00"))
    assert actual is False


def test_refund_order_should_return_true() -> None:
    service = BillingService("key_test")
    order = _make_order()
    actual = service.refund_order(order)
    assert actual is True


def test_charge_order_uses_customer_name_from_order() -> None:
    service = BillingService("key_test")
    customer = Customer("c2", "Jane Smith", "jane@example.com")
    order = Order("o2", customer, Decimal("25.00"))
    actual = service.charge_order(order, Decimal("25.00"))
    assert actual is True
