# 지원 후보·검증 범위

사용자 결정(2026-09-17): Rust 전환, macOS Apple Silicon에서 설치·운용 UX를 먼저 구현하고 Linux/systemd와 Docker/Compose를 후속으로 유지한다.

| 조합 | 현재 상태 |
| --- | --- |
| Rust 1.86.0 / macOS arm64 | 이번 CLI 개발·테스트·native 빌드 대상 |
| macOS arm64 + Java 21 + RocksDB + launchd | 첫 실제 package/UX 인수 목표. 동봉 JRE·OS 최소 버전·서비스 인수 전 |
| Linux amd64/glibc + Java 21 + RocksDB + systemd | 기존 development manifest 유지, 후속 서버 인수. Ubuntu 24.04 후보 |
| Docker/Compose | 후속 이미지·volume·network·종료/재생성 인수 필요 |
| Intel Mac·Linux ARM64·Windows·Alpine/musl·H2 제품 지원 | 별도 구현·인수 전 미지원 |

## Toolchain과 의존성

현재 설치된 Rust 1.86.0을 `rust-toolchain.toml`/Cargo의 rust-version으로 고정하고 Cargo.lock으로 전이 의존성과 checksum을 고정했다. Cargo.toml의 직접 의존성은 정확한 버전이며 lock 변경은 테스트와 함께 검토한다. fmt·Clippy·테스트가 기본 gate다. BXDL crate는 unsafe code를 금지하지만 의존 crate 내부의 unsafe 부재를 주장하지 않는다.

Rust 이식은 serde/serde_json(JSON), flate2(Rust compression backend), sha2/hex(hash), ed25519-dalek/base64(서명/PEM), cap-std(열린 directory 기준 staging), time(결과 시각)을 사용한다. tempfile은 테스트에만 사용한다. 외부 의존성 0개라는 이전 Go 구현의 특성은 더 이상 해당하지 않는다. 실제 제품 배포 시 CLI 전이 의존성도 SBOM/NOTICE에 포함하고 별도 보안·license 검토를 해야 한다.

첫 개발 빌드는 compiler/linker 및 Cargo registry 다운로드를 필요로 한다. 고객 실행은 검증된 CLI/JRE/JAR만 소비하며 개발 도구나 registry 접근을 요구하지 않는 것이 제품 목표다. CPU/OS별 executable은 별도 빌드하며 Mac에서 Linux target type-check와 Linux linking/실행을 구분한다.

## 엔진과 Mac 실행 근거

NIGO `ROCKSDB_OPERATIONS.md`와 `tasks/2026-09-07-rocksdb-storage-implementation.md`에 macOS/Darwin arm64·Java 21의 실제 RocksDB·Node 정상 재시작 근거가 있다. `PROVIDERS.md`는 macOS aarch64 native 경로·JVM os.arch·writable/loadable temp 조건을 명시한다. 이는 BXDL의 선택한 package/JRE/launchd 조합을 인수한 결과가 아니다.

동봉 Java 21의 공급자·patch·archive hash·NOTICE는 미선정이다. 개발 Mac의 Java를 재배포 대상으로 자동 채택하지 않는다. NIGO 공식 release/manifest가 없어 실제 engine lock도 미생성이다. source commit이나 fake JAR/Java fixture를 공식 공급물로 표기하지 않는다.

Manifest v1의 Mac 값 `libc=none`, `minGlibc=none`은 Linux glibc 조건이 적용되지 않는다는 뜻이다. Mac에 libc가 없다는 뜻이 아니다. Linux의 `glibc/2.34`는 기존 development 후보 계약값을 유지하며 실제 지원 보장이 아니다.

## 인수 분리

G1-M은 Mac 사용자 설치·launchd·동일 데이터 재시작·사용자 흐름 검증이다. 같은 사용자 권한으로 CLI와 engine가 실행되므로 Linux 전용 service UID와 같은 격리를 주장하지 않는다. 로그인/로그아웃·sleep·실패/재개 UX도 포함한다.

G1-L은 실제 Linux VM의 systemd·전용 UID·native·다중 host 네트워크 인수다. G1-D는 container 별도 인수다. 기존 사용자 node/data를 fixture로 사용하지 않는다. 개발 테스트와 과거 다른 revision의 evidence는 각 gate를 대신하지 않는다.

GitHub Actions는 macOS/Ubuntu의 Rust CLI fast checks를 정의한다. push/원격 실행 전이며 workflow 존재가 CI PASS·engine/service 인수의 근거는 아니다. OS runner architecture는 실행 evidence로 기록한다.
