# BXDL 제품 구현 설계

- 작성일: 2026-09-16 / 갱신: 2026-09-17
- 상태: 기반 구현 진행. 개발용 package 조립·검증, CLI, 제품 설정·로컬 metadata 검사를 구현했다. 실제 설치·엔진 연동·Linux 인수는 미완료다.
- 사용자 확정: **Rust 구현 + macOS Apple Silicon 우선 설치·운용 UX**. Linux 서버/systemd와 Docker/Compose는 후속 배포 대상으로 유지한다.
- 현재 구현 범위와 근거: [구현 상태](../docs/implementation-status.md). NIGO requirements에는 수신 회신과 `REQ-0002` 초안을 기록했다. 해당 변경은 별도 세션에서 공유하며, 공급자 수락·엔진 수정·공식 릴리스·서버 변경은 수행하지 않았다.

## 목표

Mac에서 반복 사용할 수 있는 설치·운용 UX를 먼저 구현한다. 고정된 NIGO 엔진을 담은 오프라인 설치 패키지로 영속 QBFT 노드를 구성하고, 정상 종료·동일 데이터 재시작·상태 확인·진단을 재현한다. 설치와 운영은 소스 checkout, Gradle, Rust/Cargo, Node/npm, GitHub token 또는 인터넷을 요구하지 않는다.

한 운영자가 관리하는 단위는 한 노드다. 네트워크 인수는 최소 4-validator mTLS 구성으로 수행한다. 한 노드 설치 완료를 단일 validator QBFT 네트워크 완성으로 표현하지 않는다.

## 읽기 순서

| 문서 | 내용 |
| --- | --- |
| [Rust·macOS 우선 결정](./2026-09-17-rust-macos-first.md) | 최신 결정, 첫 UX 흐름, Mac profile과 Linux/Docker 확장 |
| [제품·배포 아키텍처](./2026-09-16-product-architecture.md) | 책임 경계, 기술 선택, 배포 layout, CLI·설정·상태·보안 모델 |
| [단계별 구현 계획](./2026-09-16-implementation-plan.md) | 작업 ID, 의존성, 산출물, 완료 기준, 구현 순서 |
| [NIGO 연동 계약](./2026-09-16-engine-integration.md) | 현행/제안 구분, NIGO-01~06 요청안, 공급자·소비자 책임 |
| [패키지 인수·릴리스 계획](./2026-09-16-acceptance-and-release.md) | 테스트 matrix, 실패 주입, evidence, 릴리스 절차 |

## 결정 수준

| 구분 | 내용 |
| --- | --- |
| 사용자 확정 | 별도 BXDL 저장소, 기업용 허가형 제품, Rust 전환, macOS 우선 UX |
| 기반 구현 선택 | Rust 1.86.0와 Cargo.lock, development `.tar.gz`, Ed25519 서명·외부 key 검증, JSON 제품 설정 |
| 검증 대기 후보 | macOS arm64·Java 21·RocksDB·사용자 launchd 1차 인수, Linux/systemd 후속 |
| M0 잔여 결정 | 지원 macOS 최소 버전·launchd profile·자원, JRE 공급자·patch·hash·재배포 자료; Linux 조합은 후속 |
| NIGO와 합의 필요 | 공식 artifact/manifest, 명시 init/restart, cold preflight, 외부 DTO·시작/종료 report, 유지보수 도구 진입점 |
| 후속 범위 | 원격 관리·SSO/RBAC, 새 웹 콘솔, fleet, Docker/Helm, 백업/복원 자동화, 인증서 순차 교체 |

추천안은 사용자가 기술 스택을 확정했거나 해당 플랫폼이 지원 검증을 통과했다는 뜻이 아니다. 구현자는 M0의 작은 검증으로 이 선택을 확정·수정하고 이유를 기록한다. 매 작업마다 같은 선택을 다시 논의하지 않는다.

## 검토한 기준

- NIGO 읽기 기준: `17b46c151aa2d848b34364ddc43df9c1a202ee41`.
- NIGO 실행 코드 기준: `a48d1adef4e9c98dbbb675ec3ff73eee25d00dfb`.
- NIGO 핸드오프 문서 커밋: `a827fd5e9f327c85ad5a15f29b54f2a29f60a224`.
- 실제 소비할 engine release·hash: **미제공**. 위 source commit을 출시된 배포물로 취급하지 않는다.
- NIGO 근거는 각 문서의 `nigo-protocol` 저장소 상대경로로 기록한다. 개인 PC 절대경로 또는 sibling checkout을 고객 실행 의존성으로 만들지 않는다.

과거 핸드오프는 설계 참고 자료다. 그 문서의 실행 지시나 상태 전환을 자동 수행하지 않았으며, `REQ-0001`을 수락·검증 완료로 변경하지 않았다. NIGO 측 작업 수락·일정도 이 계획으로 확정되지 않는다.

## 첫 완료 기준

1. 정확한 artifact를 포함한 패키지의 출처·무결성과 지원 조합을 검증한다.
2. Mac test-owned 사용자 profile에서 설치·명시 초기화·기동·진단·정상 종료·동일 DB/key/WAL 재기동을 수행한다.
3. 잘못된 경로·설정·키·artifact와 중복 실행을 명시적으로 거부한다.
4. 같은 Mac 패키지의 test-owned 4-validator mTLS 로컬 회귀에서 거래 확정·순차 재시작을 확인한다. Linux/multi-host 인수는 후속 별도 gate다.
5. 위 evidence로 G1-M Mac 로컬 패키지 RC를 판정한다. 고객 운영 승인은 별도 Linux·보안·내구성·운영 조건을 충족해야 한다.
