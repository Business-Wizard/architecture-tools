from decimal import Decimal

from src.order_service import create_order, confirm_order


def test_create_order_should_start_pending() -> None:
    order = create_order("o20", "c20", "Bob", "bob@example.com", Decimal("15.00"))
    assert order.status == "pending"


def test_confirm_order_should_set_confirmed() -> None:
    order = create_order("o21", "c21", "Carol", "carol@example.com", Decimal("20.00"))
    confirm_order(order)
    assert order.status == "confirmed"
