# Mac 파일 설치와 NIGO 개발 후보 cold 소비 검증

2026-09-17, BXDL Rust 작업 트리. 기준 HEAD `a3a18d531cc4f0368559a6f1090724fcd21cbf8c` 이후 R1 setup 변경을 보존하고 R2 파일 설치·R3 cold adapter를 추가한 결과다. Git commit·원격 CI·정식 release는 이 기록의 성공 근거가 아니다.

## 공급 후보와 환경

- 읽은 NIGO HEAD: `17c0bc3b63756915c18fa2942afdd73426bb2eac`(PR #117).
- 소비 JAR source: `aee1cbe5e0383ea9eb4d3334d72e3ea23d1d8b27`, dirty=true, version `0.0.1-SNAPSHOT`.
- JAR: 159,965,726 bytes, SHA-256 `37070fbddaf1350b81952b9f3bd718b76f246557d7cd5abe2ec580a13d8de5fe`.
- Contract fingerprint: `0ac719e0175e9d5fb2e732e297b496e5725bd17f18d84bb53c6b51c235fd71cc`.
- OS: macOS 26.6.2(25G83), arm64. Rust 1.86.0와 기존 lock 유지, 새 crate 의존성 없음.
- Java: Oracle 21.0.7+8-LTS-245. 호스트 JDK와 `jlink --add-modules java.se,jdk.crypto.ec,jdk.unsupported --strip-debug --no-header-files --no-man-pages`로 만든 **로컬 시험용 runtime**에서 cold 명령을 확인했다. 고객용 JRE·재배포 승인·완전한 SBOM 선정이 아니다.

NIGO build 출력 manifest/checksum, 내장 engine-build와 계약 4개 entry/hash/aggregate를 정확한 JAR와 대조했다. 공급자의 기존 source/JAR 테스트는 별도 근거이며 BXDL 실행으로 재표기하지 않았다. [원문 snapshot](../contracts/nigo/development-2026-09-17/README.md)을 보존한다.

확인 중 NIGO의 같은 HEAD 작업 트리에 managed runtime recovery·shutdown·sync 문서/소스 수정이 추가됐다. 새 계약의 bytes는 위 후보와 다르다. 이 진행 중 변경을 기존 JAR에 포함됐다고 가정하지 않으며, 새 canonical 산출물과 제공자 시험을 받은 뒤 별도 검증한다. NIGO를 빌드하거나 수정하지 않았다.

## 코드 검증

`make check build build-macos check-linux` 통과. Cargo는 기존 cache를 사용한 offline/locked 모드로 실행했다.

| 검증 | 결과 |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets -- -D warnings` | PASS |
| 전체 Rust 테스트 | **107 PASS**, 실패·무시 0 (lib 87, CLI 8, package/config 통합 2, setup CLI 5, setup 안전 5) |
| host release + aarch64-apple-darwin release | PASS |
| x86_64-unknown-linux-gnu | type-check PASS. Linux linking/실행 인수는 아님 |

새 설치 회귀 10건은 서명·platform·Mac 별칭·기존 경로·동시 설치·후기 payload/hash/gzip CRC/추가 member/trailer·소유 receipt 정리·열린 동일 archive stream 사용을 검사했다. 엔진 회귀 13건은 pin/identity·strict JSON·secret 제거·환경 분리·설정 snapshot/변경 감지·경로 별칭·출력 한도·timeout kill/reap·EOF 없는 지속 출력의 제한 시간을 검사했다. 기존 setup/artifact 테스트도 함께 통과했다.

독립 리뷰에서 종료 후 지속 출력이 reader join을 지연하는 경계와 Mac `/private/TMP` 별칭의 data overlap을 수정했다. 초기 timeout 시험의 shell PID 파일 생성 가정은 실제 spawn PID 관찰 및 barrier 기반 trickle 시험으로 교체했다. timeout 기준을 완화해 성공 처리하지 않았다. 정식 JVM timeout은 1~120초 옵션이며 이 독립 회귀는 100ms 시간 제한과 직접 child의 회수를 확인한다.

## 실제 package 소비

테스트 전용 공백 경로와 INSTANT/DEV_INSTANT chain을 만들고 native node.json의 backend를 rocksdb로 지정했다. 이 선택은 cold 검사 입력일 뿐 QBFT 고객 기본값 또는 RocksDB native open 성공이 아니다. 기존 사용자 node/data/PKI를 사용하지 않았다.

1. 호스트 Java와 exact JAR의 `engine inspect`: exit 0, AVAILABLE 및 lock 전체 identity 일치.
2. 같은 후보의 `engine preflight`: BXDL exit 5/INCOMPLETE, 원래 NIGO exit 3 보존. CONFIGURATION=PASS, KEY_MATERIAL=NOT_APPLICABLE, DB/WAL·PORTS_AND_PEERS·NATIVE_RUNTIME=NOT_CHECKED.
3. 실제 JAR·시험용 JRE·이번 Rust CLI를 development package로 조립·자체 verify하고 새 폴더에 install. 설치된 CLI와 runtime으로 위 두 명령 실행.
4. 같은 destination 재설치 거부, 기존 receipt bytes 보존. data 폴더가 생기지 않고 원래 node/chain bytes가 그대로임을 검사.
5. 잘못된 native backend는 ENGINE_INVALID_CONFIGURATION, 기대 dirty identity 불일치는 ENGINE_IDENTITY_MISMATCH, 잘못된 JAR pin은 ENGINE_PIN_MISMATCH로 거부.


최종 시험 archive는 180,877,177 bytes, SHA-256 `f9e7835c31995ad1afb3a883784915e804eed1bb7f7db3831af399f40e824326`이며 138개 파일/248,147,457 payload bytes를 설치했다. 이 시험 archive에 들어간 host release CLI SHA-256은 `0d63004d7b6684acdf5b4f794db416c632646655e104d3b68269499ca59e969d`다. 명령은 PATH를 빈 값으로 실행했다. 설치·정보조회 exit 0, cold exit 5, 기존 대상 충돌 exit 3을 확인했다.

시험 package의 NOTICE/SBOM 파일은 LOCAL_TEST 표기가 있는 입력 목록이다. 필수 payload 존재·무결성 확인과 법적/공급적 충분성 검증을 구분한다. JAR/JRE/archive/시험 lock·설정은 커밋하지 않는다. 실제 설치/cold 소비에는 Cargo/Gradle/npm/NIGO source 접근이 필요 없었지만 네트워크 격리 OS의 전체 오프라인 G1-M 인수는 별도다.

## 남은 인수

init/resume-init/run/stop·launchd·QBFT key/mTLS·거래·동일 DB/WAL 재시작·4-validator·RocksDB/native load는 실행하지 않았다. OS syscall 전수 계측이나 일반 crash/power-cut 내구성도 미실행이다. 선택한 공식 JRE/NOTICE/SBOM, Apple signing, 최소 macOS 지원, Linux/systemd·Docker 인수는 후속이다. 파일 receipt와 cold INCOMPLETE를 정상 노드·전체 R2/R3/G1-M 완료로 올리지 않는다.
