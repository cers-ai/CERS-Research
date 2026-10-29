import pytest

from chengsi_worker import UnsupportedPythonError, assert_supported_python


def test_accepts_locked_python() -> None:
    assert_supported_python((3, 13))


@pytest.mark.parametrize("version", [(3, 12), (3, 14), (3, 11)])
def test_rejects_other_python(version: tuple[int, int]) -> None:
    with pytest.raises(UnsupportedPythonError):
        assert_supported_python(version)
