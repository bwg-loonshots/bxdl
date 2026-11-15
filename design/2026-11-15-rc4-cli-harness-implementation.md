# RC4-04 — 외부 관리 CLI와 고정 런타임 인수 도구

- 선행: [RC4-03 고정 A·R](../results/2026-11-15-rc4-runtime-assembly.md).
- 범위: RC.3/RC.4 인증 profile·응답 검증, 외부 관리 CLI B, 고정 패키지용 서비스·네트워크 인수 도구.
- 실제 제품 설치·init·LaunchAgent는 RC4-05, 네트워크·아래 RPC 사례는 RC4-06이다. 구현과 모의 회귀를 실물 PASS로 승계하지 않는다.

## 인증과 빌드 경계

`executor/profiles.rs`는 RC.3/RC.4의 archive·manifest·내부 실행기 hash/크기·JAR·Java·제품 revision·inventory 수/크기와 수신 engine-info 전체를 고정한다. 서명과 실제 전체 inventory를 먼저 확인한 뒤 같은 profile의 binding·응답 identity를 대조한다. source뿐 아니라 Console/계약/dirty/channel/Java major가 달라도 거절한다. 알려진 archive의 manifest·identity 혼합은 다른 후보나 native 경로로 자동 전환하지 않는다. 알 수 없는 archive에는 기존 same-CLI·worker 검사를 유지하며 임의 실행기에 위임하지 않는다.

기존 `BXDL_RC3_EXECUTOR_V1`/`executor.rc3-legacy-v1`을 유지하고 `BXDL_RC4_EXECUTOR_V1`/`executor.rc4-v1`을 추가한다. 새 B의 install/update/rollback 후보는 두 capability를 모두 요구한다. 사용자별 노드 목록만으로 지원 중단이 안전하다고 추정하지 않는다. 이미 배포된 구형 bootstrap의 정책까지 바꾸는 기능은 아니다. 변경 작업 dispatch 후 결과를 확정하지 못하면 UNKNOWN을 유지하고 재시도하지 않는다.

A는 `d38027a474f1e8d4fb7b1d38ea17972ba88d93af`, R은 archive `a9f67988e785260b35504bd79a19df4a4820e72eafbf537328ca2e31f7e16f60`으로 유지한다. B는 구현을 검증·커밋한 별도 clean source에서 빌드한다. B의 source/binary/관리 bundle identity는 실행 후 결과에 기록한다. B를 R의 A에 덮어쓰지 않으며 새 지원 profile 추가 때문에 R을 재조립하지 않는다. 서비스가 사용하는 worker는 계속 R의 A다.

## 인수 입력과 실행 방식

공유 `runtime_profiles.py`가 후보별 고정 R·검증 공개키·engine lock·실행기·JAR/Java·manifest inventory를 관리한다. 파일 수·총 크기는 실제 manifest에서 산출한 값과 profile의 기대값을 대조한다. RC.3은 270파일, RC.4는 269파일이며 같은 숫자로 일반화하지 않는다.

일반 `run_acceptance.py`의 RC.4 경로는 고정 R·공개키와 내부 A를 입력받아 읽고 검증한다. 재조립용 spec/payload/key 생성 도구를 받지 않는다. RC1~3의 기존 조립·인수 경로는 보존한다. 외부 B의 이름 기반 운용·업데이트·복귀는 관리 전용 runner에서 검증하며 일반 runner 결과로 대신하지 않는다.

서비스 준비 도구는 RC4-02 fixture를 준비 guard로 검증한 뒤 n1의 chain·자격증명을 새 private 경로에 복사한다. 원본 chain의 4-validator membership은 유지한다. 새 instance/data/port와 수동 peerless 설정을 만들어 단일 프로세스의 `MANUAL_START_INITIALIZED`를 검사할 수 있게 한다. 이 설정은 합의 네트워크를 1-validator로 바꾸는 작업이 아니다. CLI/JVM/key generator·init·서비스를 실행하지 않으며 원본 불변·새 data 부재·후보 및 복사본 hash/provenance를 기록한다. 수동 운용 입력의 실제 cold 의미 검증은 RC4-05에서 수행한다.

RC.4 서비스 runner는 해당 준비 provenance를 요구한다. 관리 네트워크 runner는 양쪽 CLI의 RC.4 capability와 고정 runtime을 검증한다. CLI 전환 전후 attempt/PID·data identity·초기화 journal·worker·payload를 비교한다. 기존 RC.3 입력·관리 CLI를 이름만 바꿔 RC.4 인수에 사용하지 않는다.

## RC4-06 RPC 선정 사례

`rc4_rpc_probe.py`는 인증한 fresh 시험망의 첫 start 후, 첫 거래 제출 전에 동작한다. RPC는 loopback으로 한정된 기존 transport를 사용하며 호출마다 5초·응답 2MiB 제한을 적용한다. 자체 예산은 52회·120초이며 각 호출 전후 확인한다. 진행 중 transport를 별도 타이머로 중단하는 전체 실행 상한은 아니다. 자동 재시도와 거래 제출은 없다.

| 사례 | 호출 수 | 고정 기대값 |
| --- | --- | --- |
| KZG 정상 call·estimate | 2 | 공급 고정 192-byte vector의 64-byte 결과, estimate 73,628 |
| KZG 잘못된 proof call·estimate, 길이 오류 estimate | 3 | `-32000`과 고정 precompile 실패 메시지 |
| 공개 `0x0105/0x0106/0x0107` × 빈 입력/내부 READ 형태 입력 × call/estimate | 12 | 일반 공개 EVM 계정의 빈 결과·Osaka calldata floor; 내부 host 응답을 노출하지 않음 |
| SHA256 control call·estimate | 2 | `abc`의 SHA-256, estimate 21,120 |
| Type-4 빈 authorizationList call·estimate | 2 | `-32602`와 고정 Invalid params 메시지 |
| 체인 identity | 1 | chainId 11578 |
| 전후 관측 5계정 × balance/nonce/code | 30 | 관측한 계정 값 일치 |
| 합계 | **52** | 기능 사례21개 + identity/state 조회31개 |

오류 응답은 정수 code·문자열 message와 JSON-RPC envelope를 엄격하게 검사한다. 원응답은 private evidence로 저장하며 공개 결과는 고정 reason·입력/응답 digest·호출 수·부분 사례를 남긴다. transport 실패·timeout·예상 불일치는 INCOMPLETE이며 후속 사례는 수행하지 않는다.

기대값은 NIGO가 공급 근거에 연결한 고정 workload를 읽어 선정했다. 아래 SHA는 **개별 workload 파일**이며 공급 evidence의 전체 harness/source proof SHA와 다르다.

| workload | 고정 commit | 개별 파일 SHA-256 |
| --- | --- | --- |
| `nigo-e2e-test-suite/scripts/kzg-signed-workload.mjs` | `dd49faf881790a40c65b261942df073bcfa6d8a7` | `72f926e9e7071180ce16fc7cb01555ce8cab237a22b37cc44746415165829930` |
| `nigo-e2e-test-suite/scripts/public-host-workload.mjs` | `60fc696b4dd39c54713b7f7f1929b613bde2da3c` | `f36909a5dd637e5b7d47590b618e02d05efe07ea7a2d48357d418aaa90ac7ab6` |
| `nigo-e2e-test-suite/scripts/type4-rpc-workload.mjs` | `4e0cefcb195e7e66fac08ae47edadd03b845ec08` | `1d383f742b944477a0be4b44ac5425a4c148255c5d927ae06f9f54e90d1358b6` |

공급자의 [여섯 runner evidence](../contracts/nigo/engine-rc4-2026-11-15/evidence/exact-jar-summary.json)는 공급 근거로 보존한다. BXDL 도구가 이를 전부 재실행하거나 모든 기능을 인수한 것으로 표시하지 않는다. 성공한 authorization의 Type-4 simulation·signed Type-4·provider fault 주입·중첩 host 호출·Native 전체 recipe는 위 21사례에 포함하지 않는다. 필요하면 별도 fixture/기대값·예산을 승인 범위에 추가하고 결과를 따로 기록한다.

Native/Console은 동일 수신 JAR의 catalog 378 payload·47 stock projection 및 fingerprint에 대한 RC4-02/03 정적 근거를 유지한다. 후속 fresh init와 실제 bootstrap/health의 전체 engine identity 검증을 별도로 연결한다. 실제 genesis DB 초기화, 제공 Console 동작과 Native recipe의 기능 인수는 정적 catalog PASS나 위 RPC subset으로 대체하지 않는다.

## 검증 및 후속

`make check`, Linux 타입 검사, RC.3/RC.4 교차·변조·미지원 profile·UNKNOWN·capability 손실·harness provenance 회귀를 수행한다. clean 관리 CLI B를 별도로 조립·검증하고, 엔진을 시작하지 않는 CLI 설치/조회 및 구형 capability 전환 거절을 확인한다. 정확한 수행 횟수·source·artifact identity·한계는 RC4-04 결과에 남긴다.

다음은 고정 R+B와 새 서비스 입력으로 RC4-05의 설치·명시 init·로그/진단·같은 DB 재시작을 검증하는 것이다. RC.3→RC.4 DB migration·혼합망 rolling upgrade·정식 고객 릴리스·LaunchDaemon·Linux 실제 운용은 이번 범위에 추가하지 않는다.
