"""Canonical publisher identity, retaining the pre-transfer release URLs."""

REPOSITORY = 'Pipapoen/pactrun'
LEGACY_RELEASES = frozenset(f'1.0.0-alpha.{n}' for n in range(1, 10))


def release_repository(version):
    return 'Doner357/pactrun' if version in LEGACY_RELEASES else REPOSITORY
