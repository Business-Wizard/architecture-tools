from src.stripe_client import StripeClient


def test_charge_should_return_stub_charge_id() -> None:
    client = StripeClient("sk_test_key")
    actual = client.charge(5000, "customer@example.com")
    assert actual == "ch_stub_ok"


def test_refund_should_return_true() -> None:
    client = StripeClient("sk_test_key")
    actual = client.refund("ch_stub_ok")
    assert actual is True
