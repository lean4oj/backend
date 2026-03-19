#include <algorithm>
#include <exception>
#include <gmp.h>
#include <lean/lean.h>
#include <set>
using std::exception, std::exception_ptr;

extern "C" {
lean_object *
l_Lean_Language_SnapshotTree_foldM___at___00main_spec__3(lean_object *,
                                                         lean_object *);
uint64_t l_Lean_Level_data___override(lean_object *);
uint64_t l_Lean_Name_hash___override(lean_object *);
size_t strlen(const char *);
ssize_t write(int, const void *, size_t);
}

void cc() {
  const char *s = 0;
  int len = 0;
  try {
    exception_ptr eptr{std::current_exception()};
    if (eptr) {
      std::rethrow_exception(eptr);
    }
  } catch (exception &e) {
    s = e.what();
    len = strlen(s);
  } catch (...) {
  }
  write(1, "\x08\x01", 2);
  write(1, &len, 4);
  write(1, s, len);
  write(1, "\x00", 1);
  std::exit(0);
}

extern "C" lean_object *protect(lean_object *arg1, lean_object *arg2) {
  std::set_terminate(cc);
  try {
    return l_Lean_Language_SnapshotTree_foldM___at___00main_spec__3(arg1, arg2);
  } catch (exception &e) {
    lean_object *s = lean_mk_string(e.what());
    lean_object *err = lean_mk_io_user_error(s);
    return lean_io_result_mk_error(err);
  }
}

extern "C" uint8_t isMalform_nat(lean_object *nat) {
  if (lean_is_scalar(nat))
    return 0;
  else if (lean_is_mpz(nat))
    return ((mpz_ptr)(nat + 1))->_mp_size < 0;
  else
    return 1;
}

std::set<lean_object *> whitelist;

extern "C" uint8_t isMalform_name(lean_object *name) {
  if (lean_is_scalar(name))
    return (uint64_t)name != 1;
  if (whitelist.contains(name))
    return 0;
  if (name->m_other != 2 || name->m_cs_sz != 32)
    return 1;
  const lean_ctor_object *c = (lean_ctor_object *)name;

  uint64_t claimed_hash = (uint64_t)c->m_objs[2];
  lean_object *subname = c->m_objs[0];
  lean_object *arg = c->m_objs[1];
  uint64_t lhs = l_Lean_Name_hash___override(subname);
  uint64_t rhs;

  switch (name->m_tag) {
  case 1: { // str
    rhs = lean_string_hash(arg);
    break;
  }
  case 2: { // num
    if (lean_is_scalar(arg)) {
      rhs = lean_unbox(arg);
    } else if (lean_is_mpz(arg)) {
      mpz_ptr r = (mpz_ptr)(arg + 1);
      if (r->_mp_size < 0)
        return 1;
      rhs = (r->_mp_size == 1 ? r->_mp_d[0] : 17);
    } else
      return 1;
    break;
  }
  default:
    return 1;
  }

  if (claimed_hash != lean_uint64_mix_hash(lhs, rhs))
    return 1;

  whitelist.insert(name);
  return isMalform_name(subname);
}

extern "C" uint8_t isMalform_level(lean_object *lvl) {
  if (lean_is_scalar(lvl))
    return (uint64_t)lvl != 1;
  if (whitelist.contains(lvl))
    return 0;
  uint64_t n = lvl->m_other;
  if (lvl->m_cs_sz != (n + 2) * 8)
    return 1;
  const lean_ctor_object *c = (lean_ctor_object *)lvl;

  uint64_t claimed_data = (uint64_t)c->m_objs[n];

  uint32_t depth = claimed_data >> 40;
  if (depth > 32)
    return 1;
  uint8_t meta = claimed_data >> 32;
  uint32_t hash = claimed_data;

  switch (lvl->m_tag) {
  case 1: { // succ
    if (n != 1)
      return 1;
    lean_object *sub = c->m_objs[0];

    uint64_t sub_data = l_Lean_Level_data___override(sub);
    if (depth != uint32_t(sub_data >> 40) + 1)
      return 1;
    if (meta != uint8_t(sub_data >> 32))
      return 1;
    if (hash != (uint32_t)lean_uint64_mix_hash(2243, (uint32_t)sub_data))
      return 1;

    whitelist.insert(lvl);
    return isMalform_level(sub);
  }
  case 2:   // max
  case 3: { // imax
    if (n != 2)
      return 1;
    lean_object *lhs = c->m_objs[0], *rhs = c->m_objs[1];

    uint64_t lhs_data = l_Lean_Level_data___override(lhs),
             rhs_data = l_Lean_Level_data___override(rhs),
             base = lvl->m_tag == 2 ? 2251 : 2267;

    if (depth != std::max<uint32_t>(lhs_data >> 40, rhs_data >> 40) + 1)
      return 1;
    if (meta != uint8_t((lhs_data | rhs_data) >> 32))
      return 1;
    if (hash !=
        (uint32_t)lean_uint64_mix_hash(
            base, lean_uint64_mix_hash((uint32_t)lhs_data, (uint32_t)rhs_data)))
      return 1;

    whitelist.insert(lvl);
    return isMalform_level(lhs) || isMalform_level(rhs);
  }
  case 4:   // param
  case 5: { // mvar
    if (n != 1 || depth)
      return 1;

    uint32_t expected_meta = 6 - lvl->m_tag;

    if (meta != expected_meta)
      return 1;

    lean_object *child = c->m_objs[0];
    uint64_t expected_hash = l_Lean_Name_hash___override(child);
    if (lvl->m_tag == 4) {
      expected_hash = lean_uint64_mix_hash(2239, expected_hash);
    } else {
      expected_hash =
          lean_uint64_mix_hash(2237, lean_uint64_mix_hash(0, expected_hash));
    }
    if (hash != (uint32_t)expected_hash)
      return 1;

    whitelist.insert(lvl);
    return isMalform_name(child);
  }
  default:
    return 1;
  }
}
