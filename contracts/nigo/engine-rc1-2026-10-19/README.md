# NIGO engine RC1 수신 — 2026-10-19

BXDL이 private GitHub prerelease에서 `nigo-engine-v0.1.0-rc.1`을 직접 내려받아 검증한 기록이다.
공급 정책의 단일 release 라인·불변 후보·명시 lock 선택·수정/backport·철회 규칙과 Mac 우선 범위를 수용한다.
원본 계약·manifest·공급자 evidence는 bytes 그대로 보존한다. 이 README와 `consumer-receipt.json`은 BXDL 기록이다.

## 선택한 후보

| 항목 | 값 |
| --- | --- |
| 공급 문서 checkout | `a078475cf82ac08dc1a188c70260b936e50bcdeb` (NIGO PR #208) |
| 인계 문서 수정 commit | `4ec4e0e600d9bd573a7f00a4d1b5cc45092c20ff` |
| 공급 라인 / 태그 | `release/engine-0.1` / `nigo-engine-v0.1.0-rc.1` |
| 엔진 source / dirty | `540a89f0814c01d2a4ec67da1087af35195882ca` / `false` |
| JAR SHA-256 / bytes | `f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45` / 164,083,923 |
| Archive SHA-256 / bytes | `4036c4bc2fd48b6f155cd5b3c746a53da3d57d04ff9da1b891213a0177f88703` / 157,696,067 |
| Manifest SHA-256 | `7e2f276266d0a42f602e1d2f2bd52f2452f8e74ed9015fc2265269b94e2664cd` |
| 계약 fingerprint | `bac1756a81f2d2ddf67ae094fc770bc67060482acab87da7d48b93d441ab8f88` |
| Console fingerprint | `4df95293576a4baaa63639d3540ae6657b89c0eae80b1e2e2f7f9e6412a32c73` |
| 공급 채널 / 계약 | `development`, `officialRelease=false`, `PROPOSED` |
| 소비자 기반 | BXDL `70f527be3d1ecf321bb661fa9202f0b4cadf7ef0` 이후 `feat/engine-rc1-adoption` 변경 |

[공급 release](https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.1)의
release ID는 `416932996`, archive/checksum asset ID는 `655339980`/`655339981`이다.
태그는 unsigned이며 release `immutable=false`다. 서버가 불변성이나 서명 provenance를 강제한다고 주장하지 않는다.
권한 있는 저장소에서 읽은 문서·태그 full commit과 다운로드한 실제 digest를 함께 대조했다.

## 수신 검증

- 원격 archive/checksum asset의 API digest·크기, 인계 문서, 로컬 공급본과 수신 bytes가 일치했다.
- 압축 경로·중복·별칭·link/special file을 검사한 뒤 별도 경로에 추출했다. 14개 파일 중 `SHA256SUMS`를 제외한
  13개 inventory가 빠짐없이 일치했고 gzip checksum을 확인했다.
- 원본 manifest·JAR build·engine-info, console, 계약 4개가 서로 일치했다. 계약/native 운영 문서의 원문을
  태그 source와 비교하고 `sourcePath + NUL + bytes + NUL` 순서로 계약 fingerprint를 재계산했다.
- 공급자 case별 결과를 다시 집계해 source 1,255 PASS·기존 opt-in NOT_RUN 2건, exact-JAR 9 PASS를 확인했다.
  이 수치는 공급자의 시험 결과이며 BXDL 제품 시험 수가 아니다.
- 상세 정적 판정은 [consumer-receipt.json](./consumer-receipt.json)에 있다. 해당 파일의 JVM/package 미실행은
  정적 수신 단계의 범위다. 후속 소비자 실행은 [RC1 인수 기록](../../../results/2026-10-19-engine-rc1-adoption.md)에 별도로 기록한다.

## 보존한 원문

상대 링크는 원래 NIGO 저장소 또는 공급 bundle 기준이다. 원문 링크나 상태를 BXDL에 맞춰 수정하지 않았다.
`supplier-README.md`와 `supplier-SHA256SUMS`는 원래 공급 bundle의 README와 inventory이며, JAR를 제외한 이 snapshot의 inventory가 아니다.
JAR·archive·JRE·PKI·DB·키·비밀번호·미정제 로그는 Git에 넣지 않는다.

| 파일 | SHA-256 |
| --- | --- |
| [ENGINE_CONTRACT.md](./ENGINE_CONTRACT.md) | `f52c8ef1c31fa51b269da547139e72ba90e6131198e351ee3426b760b5e3db95` |
| [contract.json](./contract.json) | `bffe91e25a3c58db7626fadbb8333b5216ab293b7760dee033f2fdf974841687` |
| [ENGINE_RUNTIME_CONTRACT.md](./ENGINE_RUNTIME_CONTRACT.md) | `ec52f844e9291f6a3cd17a4d93e58a71c2acd651e1706c9c7f9b53b6842cf43e` |
| [health-cases.json](./health-cases.json) | `c64d4003e8b7dba63845f17dfe1386546988640e1af61fba478206c83402c01e` |
| [engine-manifest.json](./engine-manifest.json) | `7e2f276266d0a42f602e1d2f2bd52f2452f8e74ed9015fc2265269b94e2664cd` |
| [PROVIDERS.md](./PROVIDERS.md) | `b22675857989d681dc1bbb325445e56f92476732f545e2eb610dbe0acc9f28c9` |
| [ROCKSDB_OPERATIONS.md](./ROCKSDB_OPERATIONS.md) | `e82ae404321e69dcb6d8606b5397a38457f81d7420a83d28c5b074382bbd6286` |
| [evidence/supplier-evidence.json](./evidence/supplier-evidence.json) | `632ac56fb3692128de2bb3127cf2b49076967fb0d6d3e50ce30139233cd2bf9f` |
| [evidence/test-summary.json](./evidence/test-summary.json) | `db9c9290956ddbd58336f6a917ad5c6c9fdd3070511515ad3cc2bdb7a182fe79` |
| [evidence/engine-info.json](./evidence/engine-info.json) | `78e719940c17e3b35d290b01e4f7bce6aa7d760028020b70b8e947790824ff38` |

## 계약 변화와 사용

기존 [303e 후보](../development-clean-2026-09-18/README.md)는 이력으로 유지한다. RC1는
`nigo.protocol.native-assets.registration-mode`에 정확한 `OPEN` 또는 `ALLOWLISTED`를 요구한다.
`OPEN`은 빈 목록에서 공개 등록 자격, `ALLOWLISTED`의 빈 목록은 신규 등록 거절을 뜻한다.
모드 누락·OPEN의 비어 있지 않은 목록·중복/잘못된 주소는 엔진에서 거절한다. 모드와 목록은 genesis에 고정된다.
이 등록 자격은 개별 자산의 mint/burn 권한이나 RPC/P2P 접속 권한과 별개다.

runtime 문서·정의 JSON·health fixture는 303e와 동일하다. 계약 문서와 source/console/JAR identity는 바뀌었으므로
[RC1 lock 예시](../../../packaging/engine-lock.rc1-development.example.json)와
[RC1 package spec 예시](../../../packaging/package-spec.rc1.example.json)를 사용한다.
예시의 Java hash/공급자/버전은 미선정 값이다. 실제로 선택·검증한 JRE의 launcher hash와 전체 runtime inventory를 별도로 고정한다.
후보명을 근거로 내장 `0.0.1-SNAPSHOT` 버전이나 PROPOSED 상태를 바꾸지 않는다.

설치 계획은 pin된 native/chain에서 등록 모드와 목록의 의미를 읽는다. 기존 journal 형식과 hash는 유지하고,
모드 누락을 OPEN 또는 RC1 호환으로 추론하지 않는다. 입력 요약은 엔진 cold 검증이나 등록 거래 성공의 대체가 아니다.

## 책임과 후속

BXDL이 source-free QBFT 설정·PKI 준비와 거래 제출/동일 높이 비교를 연결하는 제품 흐름을 담당한다.
NIGO는 canonical chain/key/합의 검증과 공급 계약을 소유한다. 현재 제품 생성기는 미구현이며, 이번 시험의 준비된
시험용 NGVK·새 TLS 자료를 고객용 kit 또는 4-validator 제품 인수로 표시하지 않는다.

첫 소비 범위는 Mac arm64·Java21·RocksDB·fresh data의 설치/LaunchAgent lifecycle이다. 기존 303e DB를 교체하지 않는다.
4-validator 실제 제품 합의·오프라인·중단/실패 전체 G1-M, 정식 JRE/최소 OS/NOTICE/SBOM, Linux/systemd·Docker,
기존 DB upgrade/downgrade와 정식 운영 출시는 별도다. REQ-0003 전체 VERIFIED를 주장하지 않는다.
