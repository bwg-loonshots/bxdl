# Linux native build·패키지 고정 준비 — 2026-11-17

**상태: INPUT_KIT_PREPARED_NATIVE_EXECUTION_PENDING.** LNX-04는 `ba4d5af6b505ea880be034b73094ac8c92018d57`로 커밋했다. 이 commit의 제품 source·Cargo.lock·toolchain·선정 Linux JRE/RC.4 lock과 별도 고정 빌드 도구를 하나의 전달 키트로 만들었다. 실제 Linux 머신을 실행할 수 있는 환경은 없어 native compile/link·runtime/CLI archive 조립·Linux 서비스 인수는 **NOT_RUN**이다. 이번 후속 구현은 미커밋 상태이며 push·PR·NIGO 수정은 수행하지 않았다.

[구조화 근거](./2026-11-17-linux-native-build-preparation.json) · [실행 안내](../docs/linux-native-build.md) · [입력 계획 사본](../packaging/linux-native-build-plan-20261117.json)

## 구현

- `prepare-linux-build.py`: 지정한 실제 Git commit에서 source archive와 lock을 추출한다. dirty checkout과 고정 제품 source를 혼합하지 않는다. 공급물 원본 hash·size, symlink/권한, 새 출력 경로를 검사하고 plan·선택적 delivery archive를 생성한다.
- `linux_build_contract.py`: 외부 plan SHA-256, exact inventory, source commit·lock 결속, 허용된 Git PAX comment와 제한된 source 해제, ELF64 x86_64와 GLIBC VERSION_NEEDS를 검사한다.
- `build-linux-candidate.py`: 실제 Ubuntu 24.04 amd64·비root·Rust 1.86.0 환경에서만 실행한다. 고정 source를 새 target에 빌드하고 ELF/glibc를 확인한 후 unsigned development runtime R-L와 독립 CLI bundle을 조립·재검증한다. A와 C는 최초 동일 bytes이며 archive·역할을 분리한다. Java·노드·sudo·systemd는 실행하지 않는다.
- `receive-linux-candidate.py`: 외부 plan/receipt/local CLI hash를 받아 로컬 OS용 신뢰 CLI로 archive를 재검증한다. payload·JRE 전체 inventory·JAR·manifest·A/C/Java identity를 대조하고 수신 실행 파일을 실행하지 않는다. 빌더의 보고와 수신자의 실행 증명을 구분한다.

기존 Unix 테스트가 Mac 패키지·사용자 제어 폴더를 전제한 부분을 Mac 전용으로 명시했다. 기존 25개 공통 사례는 Linux/Mac 실행을 유지하며 Linux 전용 CLI 테스트 8개를 추가했다. 순차 문답 재개·조회/수정·writer lock·고정 identity·미검토 apply·사용자 소유 관리자 descriptor 거절을 다룬다. 현재 Mac에서는 이 8개를 실제 실행하지 않았으며 Linux all-target 타입 검사만 수행했다.

테스트 정리 중 Linux `setup edit`의 잘못되거나 고정된 필드를 입력 읽기 전에 usage 오류로 거절하도록 보완했다. setup 입출력 실패도 Mac과 같은 exit 7로 분류한다. **이 두 보완과 새 테스트는 `ba4d5af` source 키트에 포함되지 않는다.** 다음 제품 source commit을 정한 후 새 키트로 갱신해야 한다. 도구는 별도 내용 해시로 고정했으며 위 commit에 포함됐다고 주장하지 않는다.

## 실제 준비한 전달물

| 항목 | 관측값 |
| --- | --- |
| archive | `dist/bxdl-linux-native-build-kit-20261117.tar.gz` — Git 미추적 생성물 |
| archive SHA-256 | `582a8cfb97933c5fb16280fffe8d7c49fd2eec97fe22095933c182da287b3ce3` |
| archive 크기 | 210,702,283 bytes |
| plan SHA-256 | `1491ffa33af8a9049af03cbe9700f1bc51699c778746d8ed5533cf69f8c57e29` |
| source archive SHA-256 | `1dc1dee7bc85c81a9ad78582e40f50785838d83d83dbed8abc6910dcbbd56631` |
| source inventory | 434개 일반 파일, 9,231,223 bytes |
| JRE archive SHA-256 | `a535a1df9c6284d9618872984e026ae0352d1f7e9e0605425c0e6114f47351dd` |
| RC.4 JAR SHA-256 | `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` |

새 delivery archive의 8개 파일(plan 포함)을 별도로 읽어 entry 종류·이름·중복·mode·size·hash와 exact inventory를 대조했다. 외부 archive hash·size, plan 사본 bytes도 일치했다. 기존 LNX-02 키트와 선정 공급물은 바꾸지 않았다. 최초 출력 경로가 Mac의 `/var` symlink 별칭을 포함했을 때 `OUTPUT_PARENT_UNSAFE`로 거절했고, 실제 `/private/tmp` 경로에서 새 키트를 생성했다.

## 검증

| 검사 | 결과 |
| --- | --- |
| 준비/공유 계약/빌드/수신 Python 신규 검사 | 52 PASS — 합성 자료·mock receipt, 실제 Linux 인수 자료 아님 |
| 전체 `RUST_TEST_THREADS=4 make check` | PASS — Rust 709, Python 257 PASS·기존 opt-in 2 SKIP (전체259), fmt·Clippy PASS |
| Linux GNU x86_64 all-target Clippy 및 `make check-linux` | PASS — 타입 검사, link/run 아님 |
| 실제 고정 source Git archive 검증 | PASS — commit/PAX·source inventory·lock 결속 |
| 실제 공급물을 사용한 키트 제작과 별도 delivery audit | PASS — 전체 8개 파일·외부 archive·plan 사본 일치 |
| Mac에서 native builder 호출 | 예상 거절 PASS — exit 3 `NATIVE_LINUX_X64_REQUIRED`, 출력 경로 미생성 |
| 실제 Linux native build / runtime·CLI archive | NOT_RUN / NOT_ASSEMBLED |
| 실제 Linux Java/cold/native/RocksDB/init/systemd/SSH 단절/DB 재시작/CLI 전환 | NOT_RUN |
| Linux exact executor profile·최소 OS·빌드 재현성 | NOT_ADMITTED / NOT_TESTED / NOT_TESTED |

테스트의 합성 ELF·receipt 및 CLI 응답 mock은 검증기 동작을 확인하기 위한 것으로, 실제 Linux 생성물의 해시나 빌드 성공을 대신하지 않는다. 수신기의 정상 경로도 실제 Linux 반환물이 오기 전에는 합성 계약 검사 범위다.

## 다음 실행

테스터가 SSH 정보를 공유할 필요는 없다. 실제 Ubuntu 24.04 x86_64 장비에서 키트 명령을 직접 실행하고 `runtime.tar.gz`, `management-cli.tar.gz`, `receipt.json`과 별도 receipt SHA-256을 전달하면 된다. 이를 수신한 뒤 실제 archive·manifest·A/C/Java/JAR hash를 확정하고 cold/native·새 설치·systemd·SSH 단절·같은 DB 재시작·CLI 전환을 인수한다.

현재 `setup`은 flat JRE/JAR와 관리자 helper를 사용하며 runtime archive 직접 설치는 Linux에 아직 연결되지 않았다. 후보 archive 생성·hash 고정만으로 Linux 설치 profile을 승인하지 않는다. 선정 JRE notices/SBOM과 제품 전체 법적 산출물의 완결성도 구분한다. 새로운 NIGO 선행 구현 요청은 없으며 실제 Linux 엔진 결함·계약 불일치가 관측되면 별도 근거로 요청한다.
