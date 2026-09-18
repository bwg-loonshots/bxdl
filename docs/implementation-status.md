# 구현 상태 — 2026-09-18

Rust·macOS Apple Silicon 우선 방향을 유지한다. R1 설정 초안 이후 **R2의 Mac 새 폴더 패키지 설치**와 **R3의 NIGO 개발 후보 정보·cold 사전검사**를 추가했다. clean NIGO 후보를 별도로 수신하고 제품 설정과 명시 native QBFT 설정을 연결하는 preflight를 추가했다. setup에서 설치·초기화·서비스 운용까지 이어지는 전체 UX는 아직 진행 중이다.

## 실제 제공 기능

- `package build/verify`: 기대 hash·결정적 tar/gzip·외부 Ed25519 key·전체 파일 inventory·경로/형식/한도 검사.
- `install`: Mac arm64 개발 package를 새 명시 경로에 설치. 기존 대상 거부, 검증 중 파일 비실행, 최종 gzip 검사 뒤 receipt 기록. 중단 폴더를 자동 덮어쓰기·삭제·resume하지 않음.
- `engine inspect`: 신뢰 lock의 정확한 JAR/Java hash 및 engine-info build/source/console/contract identity 대조.
- `engine preflight`: 명시 NIGO native config/chain을 입력받는 cold JVM 검사. timeout/출력 한도·환경 분리, engine exit 3을 BXDL INCOMPLETE/exit 5로 보존.
- `config validate`/기본 `preflight`: BXDL 제품 설정·로컬 metadata 검사. 엔진 옵션이 없는 preflight는 엔진을 실행하지 않는다.
- 결합 `preflight`: 제품/local 검사, 명시 QBFT VALIDATOR/MTLS 설정과 ID·경로·endpoint·키 참조 일치, 같은 native snapshot의 pinned engine cold 검사. 제품/설정/chain hash와 결과를 연결한다. 로컬 FAIL·불일치는 JVM 실행 전에 거부한다.
- `setup`: 14개 제품 설정 입력·수정·checkpoint·재개·기존 JSON 가져오기·새 JSON 출력. 파일 저장 성공은 engine/service 준비 완료가 아니다.

`init/start/stop/status/logs/diagnose/upgrade/uninstall`·launchd는 미구현이다. 제품 config에서 NIGO QBFT peer/pin/validator 상세 설정을 추측 렌더링하지 않는다. `setup`의 instance.json과 `engine preflight`의 node.json을 구별한다.

## 새로 수신한 NIGO 결과

문서 HEAD `49d1cefc`(#119)에서 clean 인계 자료를 읽었다. source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false인 실제 JAR `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455`를 별도로 검증했다. [새 원문·공급자 증거 snapshot](../contracts/nigo/development-clean-2026-09-18/README.md)과 [소비자 실행 결과](../results/2026-09-18-clean-engine-preflight.md)를 따른다. 기존 dirty 후보 snapshot·결과는 당시 이력으로 보존했다.

#118의 QBFT drain·fresh sync·genesis-only observer·실제 source 종료 후 재선택 보완과 clean JAR 공급은 제공자 근거에서 확인됐다. BXDL에서는 같은 clean JAR를 담은 시험 Mac package를 새로 설치하고 **한 validator의 실제 QBFT cold 설정·NGVK/TLS 자료 검사**까지 확인했다. peer 없는 정적 fixture이며 실제 4-node 운영·DB/native·launchd·G1-M은 수행하지 않았다. NIGO 저장소·요구 원장은 변경하지 않았고 REQ-0002 OPEN/계약 PROPOSED를 유지한다.

## 계획 대비 상태

| 작업 | 상태 | 남은 조건 |
| --- | --- | --- |
| BX-001/005 | IN_PROGRESS | 첫 Mac profile, 최소 OS/JRE/native·launchd 인수 |
| BX-002/010 | DONE | Rust 기반·CLI envelope·고정 toolchain/lock |
| BX-003 | PLAN | 정식 Java21 공급자·patch/hash·NOTICE/SBOM 선정 |
| BX-004 | IN_PROGRESS | clean 후보 수신·검증 완료, 전체 계약 합의/제품 인수 후속 |
| BX-011~014 | IN_PROGRESS | build/verify/새 폴더 설치 구현, 정식 공급·릴리스/업데이트 후속 |
| BX-020 | IN_PROGRESS | v1 제품/native 명시 설정 대조 구현, 자동 렌더·network/peer/pin 입력 확장 후속 |
| BX-021/022/024 | PLAN | NIGO 명시 init/resume/existing run 계약은 수신, BXDL instance journal·호출·상태 연결 필요 |
| BX-023 | IN_PROGRESS | 제품/native 결합 cold 검사 및 실제 QBFT 키 자료 확인, DB/native/service 인수 미완료 |
| BX-030 | IN_PROGRESS | R1 초안 + R2 파일 installer, setup package 선택·instance 등록/중단 재개 후속 |
| BX-031~034 | PLAN | launchd·상태/진단·제거. QBFT 종료 증명과 결합해 인수 |
| BX-040~043 | PLAN | G1-M 전체 사용자 흐름·같은 데이터 재시작·4-validator. Linux/Docker 별도 후속 |
| BX-044 | IN_PROGRESS | 로컬 fmt/clippy/test와 Mac 빌드; 이번 원격 CI 미실행 |
| BX-050~053/M6 | PLAN | 버전 쌍 업데이트·offline 유지보수·고객 운영 확대 |

## 다음 순서

1. setup에 검증한 package 선택과 NIGO native 설정 준비를 연결한다. 이번 명시 native 입력 방식 위에 QBFT membership·peer/pin·PKI/거래 fixture를 준비한다. 자동 렌더 도입 시에만 새 schema/초안 migration을 설계한다.
2. engine.lock·제품 instance journal과 NIGO init/resume-init 결과를 연결한다. 명령 exit/stdout/동일 attempt report가 일치할 때만 초기화 완료로 기록한다.
3. 수신한 clean 후보의 종료·동기화 계약으로 launchd start/status/stop을 구현한다. UNKNOWN 뒤 자동 재시작하지 않는다.
4. 선정 JRE와 같은 Mac package에서 G1-M 전체 설치/운영·4-validator 회귀를 인수한다. Linux/systemd G1-L, Docker G1-D는 뒤에서 별도 검증한다.

R2 파일 receipt는 instance 등록·NIGO journal이 아니며 전체 R2/R3 완료라고 표시하지 않는다. 현재 Mac 사용자 profile의 CLI·엔진은 같은 UID다. 파일 권한만으로 강한 격리를 보장하지 않는다. 기존 사용자 node/data/key는 시험하지 않는다.
